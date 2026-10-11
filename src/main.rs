//! Tray app: battery % as the icon, ETA + mode in the tooltip, mode / report rate in the menu.

#![windows_subsystem = "windows"]

use std::{cell::RefCell, mem, ptr, sync::OnceLock};
use vaxee_tool::{i18n::Strings, *};
use windows_sys::Win32::{
    Foundation::*,
    Globalization::GetUserDefaultUILanguage,
    Graphics::Gdi::*,
    System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW},
    System::Registry::*,
    System::Threading::CreateMutexW,
    UI::Shell::*,
    UI::WindowsAndMessaging::*,
};

const WM_TRAY: u32 = WM_APP + 1;
const POLL_MS: u32 = 60_000;
const ID_COMPETITIVE: usize = 1;
const ID_STANDARD: usize = 2;
const ID_REFRESH: usize = 3;
const ID_EXIT: usize = 4;
const ID_AUTOSTART: usize = 5;
const ID_MOTION_SYNC: usize = 6;
const ID_RATE_BASE: usize = 10; // + rate index 1-5
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "VaxeeTool";

struct App {
    status: Result<Status, Error>,
    icon: HICON,
    low_alerted: bool,
}

thread_local! {
    static APP: RefCell<App> = const { RefCell::new(App { status: Err(Error::NotFound), icon: ptr::null_mut(), low_alerted: false }) };
}

/// UI strings for the Windows display language, chosen once at startup.
fn t() -> &'static Strings {
    static STRINGS: OnceLock<&'static Strings> = OnceLock::new();
    STRINGS.get_or_init(|| {
        std::env::args()
            .find_map(|a| a.strip_prefix("--lang=").and_then(i18n::lang_from_arg))
            .unwrap_or_else(|| i18n::lang_from_langid(unsafe { GetUserDefaultUILanguage() }))
            .strings()
    })
}
static mut TASKBAR_CREATED: u32 = 0;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Copy into a fixed NUL-terminated WCHAR field, truncating to fit.
fn copy_wide(dst: &mut [u16], s: &str) {
    let max = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(max)) {
        *d = c;
    }
}

/// "Battery 20%-30%  ~11-15 h left", mirroring vcc's battery panel.
fn battery_line(s: &Status) -> String {
    let t = t();
    let (label, eta) =
        match battery_estimate(s.mouse_pid, s.battery, s.charging, s.standard, s.rate) {
            Some((label, "")) => (label.to_string(), t.charge_now.to_string()),
            Some((label, h)) => (label.to_string(), t.eta_hours.replace("{h}", h)),
            None => (format!("{}%", s.battery), String::new()),
        };
    let charging = if s.charging { t.charging_suffix } else { "" };
    t.battery_line
        .replace("{label}", &label)
        .replace("{charging}", charging)
        .replace("{eta}", &eta)
}

fn mode_label(s: &Status) -> String {
    let t = t();
    t.mode_line
        .replace(
            "{mode}",
            if s.standard {
                t.standard
            } else {
                t.competitive
            },
        )
        .replace("{ms}", if s.motion_sync { t.on } else { t.off })
}

fn tooltip(status: &Result<Status, Error>) -> String {
    match status {
        Ok(s) => format!(
            "{}\n{}\n{}  {}",
            device_name(s.mouse_pid),
            battery_line(s),
            mode_label(s),
            rate_name(s.rate)
        ),
        Err(e) => format!("{}\n{}", t().disconnected, t().error(e)),
    }
}

/// Taskbar theme, so the battery outline stays visible on light and dark taskbars.
fn taskbar_is_light() -> bool {
    let mut value = 0u32;
    let mut size = mem::size_of::<u32>() as u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide("SystemUsesLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    err == ERROR_SUCCESS && value == 1
}

/// Wrap straight-alpha ARGB pixels from `icon::battery` in an HICON.
unsafe fn make_icon(size: i32, pixels: &[u32]) -> HICON {
    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // top-down, matching the pixel order
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = ptr::null_mut();
    let color = CreateDIBSection(
        ptr::null_mut(),
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        ptr::null_mut(),
        0,
    );
    ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, pixels.len());

    let mask = CreateBitmap(size, size, 1, 1, ptr::null()); // unused with 32bpp alpha
    let ii = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&ii);
    DeleteObject(mask);
    DeleteObject(color);
    icon
}

unsafe fn notify(hwnd: HWND, action: u32) {
    let size = GetSystemMetrics(SM_CXSMICON);
    let outline = if taskbar_is_light() {
        0x202020
    } else {
        0xFFFFFF
    };
    let (tip, pixels) = APP.with_borrow(|app| {
        let pixels = match &app.status {
            // Same thresholds/colors as vcc: <20 red, 20-49 orange, else green.
            Ok(s) => {
                let fill = match s.battery {
                    0..=19 => 0xFF3B30,
                    20..=49 => 0xFFA500,
                    _ => 0x09BB3B,
                };
                icon::battery(size as usize, Some(s.battery), s.charging, outline, fill)
            }
            Err(_) => icon::battery(size as usize, None, false, 0x888888, 0),
        };
        (tooltip(&app.status), pixels)
    });

    let icon = make_icon(size, &pixels);
    let mut nid: NOTIFYICONDATAW = mem::zeroed();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, &tip);
    Shell_NotifyIconW(action, &nid);

    let old = APP.with_borrow_mut(|app| mem::replace(&mut app.icon, icon));
    if !old.is_null() {
        DestroyIcon(old);
    }
}

// ponytail: HID calls run on the UI thread (~0.6s per refresh); move to a worker if the menu ever feels laggy.
fn refresh(hwnd: HWND) {
    let status = Device::open().and_then(|d| read_status(&d));
    let fire = APP.with_borrow_mut(|app| {
        app.status = status;
        let Ok(s) = &app.status else {
            return false; // keep the latch while disconnected
        };
        let (fire, latched) = low_battery_alert(app.low_alerted, s.battery, s.charging);
        app.low_alerted = latched;
        fire
    });
    unsafe {
        notify(hwnd, NIM_MODIFY);
        if fire {
            show_low_battery(hwnd);
        }
    }
}

/// Tray balloon; Windows 10/11 shows it as a toast notification.
unsafe fn show_low_battery(hwnd: HWND) {
    let name = APP.with_borrow(|app| {
        app.status
            .as_ref()
            .map_or("VAXEE", |s| device_name(s.mouse_pid))
    });
    let mut nid: NOTIFYICONDATAW = mem::zeroed();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_WARNING;
    copy_wide(&mut nid.szInfoTitle, t().low_battery_title);
    copy_wide(
        &mut nid.szInfo,
        &t().low_battery_body.replace("{name}", name),
    );
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

/// Quoted path of this exe, as stored in the Run key.
fn run_command() -> String {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
    format!("\"{}\"", String::from_utf16_lossy(&buf[..n as usize]))
}

/// On only if the Run entry points at this exe, so a moved exe shows unchecked
/// and re-enabling repairs the path.
fn autostart_enabled() -> bool {
    let mut buf = [0u16; 1024];
    let mut size = mem::size_of_val(&buf) as u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(RUN_VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    let len = (size as usize / 2).saturating_sub(1); // bytes incl. NUL -> chars
    err == ERROR_SUCCESS && String::from_utf16_lossy(&buf[..len]) == run_command()
}

/// HKCU Run entry: per-user, no admin rights needed.
fn set_autostart(on: bool) -> Result<(), String> {
    let (key, name) = (wide(RUN_KEY), wide(RUN_VALUE));
    let err = unsafe {
        if on {
            let data = wide(&run_command());
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr())
        }
    };
    if err == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(t().autostart_failed.replace("{code}", &err.to_string()))
    }
}

/// Competitive => 4000Hz, Standard => 1000Hz, keeping the current Motion Sync setting.
fn switch_mode(standard: bool) -> Result<(), Error> {
    let d = Device::open()?;
    let s = read_status(&d)?;
    if standard {
        // Lower first: 2K/4K is out of range in standard mode on pre-3954 mice.
        set_rate(&d, RATE_1000)?;
        set_mode(&d, true, s.motion_sync)
    } else {
        set_mode(&d, false, s.motion_sync)?;
        // Wired mice cap at 1000Hz, so only raise the rate over the receiver.
        if d.is_wireless() {
            set_rate(&d, RATE_4000)
        } else {
            Ok(())
        }
    }
}

/// Flip Motion Sync, keeping the current mode and report rate (allowed in every mode).
fn toggle_motion_sync() -> Result<(), Error> {
    let d = Device::open()?;
    let s = read_status(&d)?;
    set_mode(&d, s.standard, !s.motion_sync)
}

unsafe fn add_item(menu: HMENU, id: usize, text: &str, checked: bool, enabled: bool) {
    let flags =
        MF_STRING | if checked { MF_CHECKED } else { 0 } | if enabled { 0 } else { MF_GRAYED };
    AppendMenuW(menu, flags, id, wide(text).as_ptr());
}

unsafe fn show_menu(hwnd: HWND) {
    let t = t();
    let menu = CreatePopupMenu();
    APP.with_borrow(|app| {
        if let Ok(s) = &app.status {
            add_item(menu, ID_COMPETITIVE, t.competitive_item, !s.standard, true);
            add_item(menu, ID_STANDARD, t.standard_item, s.standard, s.wireless);
            add_item(
                menu,
                ID_MOTION_SYNC,
                t.motion_sync_item,
                s.motion_sync,
                true,
            );

            let rates = CreatePopupMenu();
            let max = max_rate(s.gen3954, s.wireless, s.standard);
            let listed = if s.gen3954 { RATE_8000 } else { RATE_4000 };
            for r in 1..=listed {
                let allowed = r <= max;
                add_item(
                    rates,
                    ID_RATE_BASE + r as usize,
                    rate_name(r),
                    s.rate == r,
                    allowed,
                );
            }
            AppendMenuW(
                menu,
                MF_POPUP,
                rates as usize,
                wide(t.report_rate_item).as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        }
    });
    add_item(
        menu,
        ID_AUTOSTART,
        t.autostart_item,
        autostart_enabled(),
        true,
    );
    add_item(menu, ID_REFRESH, t.refresh_item, false, true);
    add_item(menu, ID_EXIT, t.exit_item, false, true);

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd); // required so the menu closes when clicking elsewhere
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        ptr::null(),
    );
    DestroyMenu(menu); // also destroys the submenu
    PostMessageW(hwnd, WM_NULL, 0, 0);

    let result = match cmd as usize {
        0 => return,
        ID_EXIT => {
            DestroyWindow(hwnd);
            return;
        }
        ID_REFRESH => Ok(()),
        ID_AUTOSTART => set_autostart(!autostart_enabled()),
        ID_COMPETITIVE => switch_mode(false).map_err(|e| t.error(&e)),
        ID_STANDARD => switch_mode(true).map_err(|e| t.error(&e)),
        ID_MOTION_SYNC => toggle_motion_sync().map_err(|e| t.error(&e)),
        id => Device::open()
            .and_then(|d| set_rate(&d, (id - ID_RATE_BASE) as u8))
            .map_err(|e| t.error(&e)),
    };
    if let Err(e) = result {
        MessageBoxW(
            hwnd,
            wide(&e).as_ptr(),
            wide("VAXEE").as_ptr(),
            MB_ICONWARNING,
        );
    }
    refresh(hwnd);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => match lp as u32 {
            WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
            WM_LBUTTONUP => refresh(hwnd),
            _ => {}
        },
        WM_TIMER => refresh(hwnd),
        // Light/dark theme switch: redraw the outline color without re-reading the mouse.
        WM_SETTINGCHANGE => notify(hwnd, NIM_MODIFY),
        WM_DESTROY => {
            let mut nid: NOTIFYICONDATAW = mem::zeroed();
            nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
        }
        // Explorer restarted: the tray icon is gone, add it again.
        m if m == TASKBAR_CREATED && m != 0 => notify(hwnd, NIM_ADD),
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}

fn main() {
    unsafe {
        // One instance per user session; the mutex lives until the process exits.
        CreateMutexW(ptr::null(), 0, wide("Local\\VaxeeTray").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            MessageBoxW(
                ptr::null_mut(),
                wide(t().already_running).as_ptr(),
                wide("VAXEE").as_ptr(),
                MB_ICONINFORMATION,
            );
            return;
        }
        SetProcessDPIAware(); // crisp icon at the real small-icon size
        TASKBAR_CREATED = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());

        let class = wide("VaxeeTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(ptr::null()),
            lpszClassName: class.as_ptr(),
            ..mem::zeroed()
        };
        RegisterClassW(&wc);
        // Hidden top-level window (not HWND_MESSAGE, which misses the TaskbarCreated broadcast).
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            wc.hInstance,
            ptr::null(),
        );

        notify(hwnd, NIM_ADD);
        refresh(hwnd);
        SetTimer(hwnd, 1, POLL_MS, None);
        // Lets you check the notification without draining the battery to 10%.
        if std::env::args().any(|a| a == "--test-low-battery") {
            show_low_battery(hwnd);
        }

        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
