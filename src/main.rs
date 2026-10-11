//! Tray app: battery % as the icon, ETA + mode in the tooltip, mode / report rate in the menu.

#![windows_subsystem = "windows"]

use std::{cell::RefCell, mem, ptr};
use vaxee_tool::*;
use windows_sys::Win32::{
    Foundation::*,
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
    status: Result<Status, String>,
    icon: HICON,
    low_alerted: bool,
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App { status: Err("尚未讀取".into()), icon: ptr::null_mut(), low_alerted: false });
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

/// "電量 20%-30%  約 11-15 小時", mirroring vcc's battery panel.
fn battery_line(s: &Status) -> String {
    let (label, eta) =
        match battery_estimate(s.mouse_pid, s.battery, s.charging, s.standard, s.rate) {
            Some((label, "")) => (label.to_string(), "請立即充電".to_string()),
            Some((label, h)) => (label.to_string(), format!("約 {h} 小時")),
            None => (format!("{}%", s.battery), String::new()),
        };
    let charging = if s.charging { "（充電中）" } else { "" };
    format!("電量 {label}{charging}  {eta}")
}

fn mode_label(s: &Status) -> String {
    format!(
        "{}模式 (Motion Sync {})",
        if s.standard { "標準" } else { "競技" },
        if s.motion_sync { "開" } else { "關" }
    )
}

fn tooltip(status: &Result<Status, String>) -> String {
    match status {
        Ok(s) => format!(
            "{}\n{}\n{}  {}",
            device_name(s.mouse_pid),
            battery_line(s),
            mode_label(s),
            rate_name(s.rate)
        ),
        Err(e) => format!("VAXEE：未連線\n{e}"),
    }
}

/// Render `text` into a small-icon-sized HICON: GDI draws white-on-black, then the
/// grey level becomes alpha so the anti-aliased digits blend on any taskbar.
unsafe fn make_icon(text: &str, rgb: u32) -> HICON {
    let size = GetSystemMetrics(SM_CXSMICON);
    let screen = GetDC(ptr::null_mut());
    let dc = CreateCompatibleDC(screen);

    let mut bmi: BITMAPINFO = mem::zeroed();
    bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    let mut bits: *mut core::ffi::c_void = ptr::null_mut();
    let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
    let old_bmp = SelectObject(dc, color);

    // Three digits ("100") need a smaller font to fit the square.
    let em = if text.chars().count() >= 3 {
        size * 5 / 8
    } else {
        size * 7 / 8
    };
    let face = wide("Segoe UI");
    let font = CreateFontW(
        -em,
        0,
        0,
        0,
        700,
        0,
        0,
        0,
        1,
        0,
        0,
        4, /* ANTIALIASED_QUALITY */
        0,
        face.as_ptr(),
    );
    let old_font = SelectObject(dc, font);
    SetTextColor(dc, 0x00FF_FFFF);
    SetBkMode(dc, TRANSPARENT as _);
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: size,
        bottom: size,
    };
    let mut t = wide(text);
    DrawTextW(
        dc,
        t.as_mut_ptr(),
        -1,
        &mut rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP,
    );
    GdiFlush();

    let px = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
    for p in px.iter_mut() {
        let a = *p & 0xFF; // grayscale AA, any channel works
        *p = (a << 24) | (rgb & 0x00FF_FFFF);
    }

    SelectObject(dc, old_font);
    SelectObject(dc, old_bmp);
    let mask = CreateBitmap(size, size, 1, 1, ptr::null());
    let ii = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&ii);

    DeleteObject(font);
    DeleteObject(mask);
    DeleteObject(color);
    DeleteDC(dc);
    ReleaseDC(ptr::null_mut(), screen);
    icon
}

unsafe fn notify(hwnd: HWND, action: u32) {
    let (tip, text, rgb) = APP.with_borrow(|app| {
        let (text, rgb) = match &app.status {
            // Same thresholds/colors as vcc: <20 red, 20-49 orange, else green.
            Ok(s) => (
                s.battery.min(99).to_string(), // "100" is unreadable at 16px; full shows 99
                match s.battery {
                    0..=19 => 0xFF3B30,
                    20..=49 => 0xFFA500,
                    _ => 0x09BB3B,
                },
            ),
            Err(_) => ("--".into(), 0x888888),
        };
        (tooltip(&app.status), text, rgb)
    });

    let icon = make_icon(&text, rgb);
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
    copy_wide(&mut nid.szInfoTitle, "VAXEE 電量過低");
    copy_wide(
        &mut nid.szInfo,
        &format!("{name} 電量即將耗盡，請立即充電。"),
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
        Err(format!("無法修改開機自動啟動設定（錯誤碼 {err}）"))
    }
}

/// Competitive => 4000Hz, Standard => 1000Hz, keeping the current Motion Sync setting.
fn switch_mode(standard: bool) -> Result<(), String> {
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
fn toggle_motion_sync() -> Result<(), String> {
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
    let menu = CreatePopupMenu();
    APP.with_borrow(|app| {
        if let Ok(s) = &app.status {
            add_item(menu, ID_COMPETITIVE, "競技模式 (4000Hz)", !s.standard, true);
            add_item(
                menu,
                ID_STANDARD,
                "標準模式 (1000Hz)",
                s.standard,
                s.wireless,
            );
            add_item(menu, ID_MOTION_SYNC, "Motion Sync", s.motion_sync, true);

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
            AppendMenuW(menu, MF_POPUP, rates as usize, wide("回報率").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
        }
    });
    add_item(
        menu,
        ID_AUTOSTART,
        "開機自動啟動",
        autostart_enabled(),
        true,
    );
    add_item(menu, ID_REFRESH, "重新整理", false, true);
    add_item(menu, ID_EXIT, "結束", false, true);

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
        ID_COMPETITIVE => switch_mode(false),
        ID_STANDARD => switch_mode(true),
        ID_MOTION_SYNC => toggle_motion_sync(),
        id => Device::open().and_then(|d| set_rate(&d, (id - ID_RATE_BASE) as u8)),
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
                wide("VAXEE 工具已在執行中，請查看系統匣。").as_ptr(),
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
