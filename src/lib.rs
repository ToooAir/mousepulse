//! VAXEE mouse HID protocol (reverse-read from vcc.vaxee.co).
//!
//! Packet (feature report 0x0E): [0x0E, 0xA5, cmd, rw, len, data...]
//! Response uses the same layout, so data starts at byte 5.

mod bat_table;
pub mod i18n;
pub mod icon;
#[cfg(windows)]
pub use hid::*;

pub const VID: u16 = 0x3057;
const USAGE_PAGE: u16 = 0xFF05;
const USAGE: u16 = 0x01;
const REPORT_ID: u8 = 0x0E;
const HEADER: u8 = 0xA5;
pub const READ: u8 = 0x01;
pub const WRITE: u8 = 0x02;
const GENERIC_RW: u32 = 0x8000_0000 | 0x4000_0000;

pub const CMD_FIRMWARE: u8 = 0x01;
pub const CMD_REPORT_RATE: u8 = 0x07;
pub const CMD_TRACKING: u8 = 0x08;
pub const CMD_MOUSE_PID: u8 = 0x0A;
pub const CMD_BATTERY: u8 = 0x0B;
pub const CMD_WIRELESS: u8 = 0x0F;
pub const CMD_CHARGING: u8 = 0x10;

/// Report rate indexes (cmd 0x07).
pub const RATE_1000: u8 = 2;
pub const RATE_4000: u8 = 4;
pub const RATE_8000: u8 = 5;

// Receiver PIDs; anything else is a mouse plugged in by cable (paramter.js DefWireless).
const WIRELESS_PIDS: [u16; 4] = [0x1001, 0x1002, 0x2001, 0x2002];

/// Protocol and device errors. `Display` is English for the CLI; the tray
/// translates them with `i18n::Strings::error`.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    NotFound,
    Open {
        pid: u16,
        code: u32,
    },
    BadReportLength {
        pid: u16,
        len: u16,
    },
    NoReply {
        cmd: u8,
    },
    /// The receiver answered but has no link to the mouse (off or out of range).
    LinkDown,
    DataTooLong,
    RateTooHigh {
        max: u8,
    },
    StandardNeedsReceiver,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::NotFound => write!(f, "no VAXEE device found (VID 0x3057, usage page 0xFF05)"),
            Error::Open { pid, code } => {
                write!(f, "PID {pid:04X}: cannot open (GetLastError={code})")
            }
            Error::BadReportLength { pid, len } => {
                write!(f, "PID {pid:04X}: unexpected feature report length {len}")
            }
            Error::NoReply { cmd } => write!(f, "cmd 0x{cmd:02X}: no valid reply after 5 tries"),
            Error::LinkDown => write!(f, "receiver has no link to the mouse (off or out of range)"),
            Error::DataTooLong => write!(f, "data too long for one report"),
            Error::RateTooHigh { max } => write!(f, "this mode supports up to {}", rate_name(*max)),
            Error::StandardNeedsReceiver => {
                write!(
                    f,
                    "standard mode is only available over the wireless receiver"
                )
            }
        }
    }
}

/// Lets the CLI keep `Result<_, String>` and use `?` on protocol calls.
impl From<Error> for String {
    fn from(e: Error) -> String {
        e.to_string()
    }
}

// Win32 HID access; the pure helpers below stay portable so tests run on any host.
#[cfg(windows)]
mod hid {
    use super::*;
    use std::{ffi::c_void, mem, ptr, thread, time::Duration};
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::*, Devices::HumanInterfaceDevice::*, Foundation::*,
        Storage::FileSystem::*,
    };

    pub struct HidInfo {
        pub path: Vec<u16>, // NUL-terminated
        pub pid: u16,
        pub usage_page: u16,
        pub usage: u16,
        pub feature_len: u16,
    }

    unsafe fn open(path: &[u16], access: u32) -> Option<HANDLE> {
        let h = CreateFileW(
            path.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            mem::zeroed(),
        );
        (h != INVALID_HANDLE_VALUE).then_some(h)
    }

    /// Reads VID/PID and top-level collection caps; access 0 works even for system-owned collections.
    unsafe fn query(path: Vec<u16>) -> Option<HidInfo> {
        let h = open(&path, 0)?;
        let mut attr: HIDD_ATTRIBUTES = mem::zeroed();
        attr.Size = mem::size_of::<HIDD_ATTRIBUTES>() as u32;
        let mut info = None;
        if HidD_GetAttributes(h, &mut attr) != 0 && attr.VendorID == VID {
            let mut pp = mem::zeroed();
            if HidD_GetPreparsedData(h, &mut pp) != 0 {
                let mut caps: HIDP_CAPS = mem::zeroed();
                if HidP_GetCaps(pp, &mut caps) == HIDP_STATUS_SUCCESS {
                    info = Some(HidInfo {
                        path,
                        pid: attr.ProductID,
                        usage_page: caps.UsagePage,
                        usage: caps.Usage,
                        feature_len: caps.FeatureReportByteLength,
                    });
                }
                HidD_FreePreparsedData(pp);
            }
        }
        CloseHandle(h);
        info
    }

    pub fn enumerate() -> Vec<HidInfo> {
        let mut out = Vec::new();
        unsafe {
            let mut guid = mem::zeroed();
            HidD_GetHidGuid(&mut guid);
            let set = SetupDiGetClassDevsW(
                &guid,
                ptr::null(),
                mem::zeroed(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            );
            if set as isize == -1 {
                return out;
            }
            for i in 0.. {
                let mut ifd: SP_DEVICE_INTERFACE_DATA = mem::zeroed();
                ifd.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(set, ptr::null(), &guid, i, &mut ifd) == 0 {
                    break;
                }
                let mut need = 0u32;
                SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &ifd,
                    ptr::null_mut(),
                    0,
                    &mut need,
                    ptr::null_mut(),
                );
                if need == 0 {
                    continue;
                }
                let mut buf = vec![0u32; (need as usize).div_ceil(4)]; // u32 for alignment
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &ifd,
                    detail,
                    need,
                    ptr::null_mut(),
                    ptr::null_mut(),
                ) == 0
                {
                    continue;
                }
                let p = ptr::addr_of!((*detail).DevicePath) as *const u16;
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                if let Some(info) = query(std::slice::from_raw_parts(p, len + 1).to_vec()) {
                    out.push(info);
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    pub struct Device {
        h: HANDLE,
        pub pid: u16,
        pub feature_len: usize,
        /// Print TX/RX bytes and retries (CLI only).
        pub verbose: bool,
    }

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.h) };
        }
    }

    impl Device {
        /// Open every VAXEE vendor collection. With the charging cable plugged in, both the
        /// receiver and the cabled mouse show up, in no guaranteed order.
        pub fn open_all() -> Vec<Result<Device, Error>> {
            enumerate()
                .into_iter()
                .filter(|d| d.usage_page == USAGE_PAGE && d.usage == USAGE)
                .map(|info| {
                    if info.feature_len < 8 {
                        return Err(Error::BadReportLength {
                            pid: info.pid,
                            len: info.feature_len,
                        });
                    }
                    let h = unsafe { open(&info.path, GENERIC_RW) }.ok_or_else(|| Error::Open {
                        pid: info.pid,
                        code: unsafe { GetLastError() },
                    })?;
                    Ok(Device {
                        h,
                        pid: info.pid,
                        feature_len: info.feature_len as usize,
                        verbose: false,
                    })
                })
                .collect()
        }

        /// Prefer the cabled mouse: once the cable is plugged in the mouse talks over USB and
        /// the receiver loses its link (reads time out), verified with NP-01S + receiver 2001.
        pub fn open() -> Result<Device, Error> {
            let mut all = Device::open_all();
            all.sort_by_key(|d| d.as_ref().map_or(2, |d| u8::from(d.is_wireless())));
            let mut last = Err(Error::NotFound);
            for d in all {
                if d.is_ok() {
                    return d;
                }
                last = d;
            }
            last
        }

        pub fn is_wireless(&self) -> bool {
            WIRELESS_PIDS.contains(&self.pid)
        }

        /// 3954 generation (api.js keys this off the HID PID: mouse by cable or 8K receiver).
        pub fn is_3954(&self) -> bool {
            matches!(self.pid, 0x1014 | 0x1015 | 0x2002)
        }

        /// 3954 sends tracking with data length 2 for reads and writes.
        pub fn read_tracking(&self) -> Result<Vec<u8>, Error> {
            self.read(CMD_TRACKING, if self.is_3954() { 2 } else { 1 })
        }

        /// Send one command and read the reply, mirroring runAPI(): 5 tries.
        pub fn transact(&self, cmd: u8, rw: u8, len: u8, data: &[u8]) -> Result<Vec<u8>, Error> {
            let mut tx = vec![0u8; self.feature_len];
            tx[..5].copy_from_slice(&[REPORT_ID, HEADER, cmd, rw, len]);
            tx.get_mut(5..5 + data.len())
                .ok_or(Error::DataTooLong)?
                .copy_from_slice(data);

            for attempt in 1..=5 {
                self.log(format!("  TX {}", hex(&tx[..12])));
                let sent = unsafe {
                    HidD_SetFeature(self.h, tx.as_ptr() as *const c_void, tx.len() as u32)
                } != 0;
                if !sent {
                    self.log(format!(
                        "  SetFeature failed (GetLastError={}), try {attempt}/5",
                        unsafe { GetLastError() }
                    ));
                } else {
                    // Writes may need >100ms before the ACK is ready; poll the reply instead of resending.
                    for _ in 0..5 {
                        thread::sleep(Duration::from_millis(100));
                        let mut rx = vec![0u8; self.feature_len];
                        rx[0] = REPORT_ID;
                        let got = unsafe {
                            HidD_GetFeature(self.h, rx.as_mut_ptr() as *mut c_void, rx.len() as u32)
                        } != 0;
                        if !got {
                            self.log(format!("  GetFeature failed (GetLastError={})", unsafe {
                                GetLastError()
                            }));
                            break;
                        }
                        self.log(format!("  RX {}", hex(&rx[..12])));
                        if rx[1] != 0 {
                            if rx[2] != cmd {
                                self.log(format!(
                                    "  warning: reply cmd 0x{:02X} != sent 0x{cmd:02X}",
                                    rx[2]
                                ));
                            }
                            return Ok(rx);
                        }
                    }
                    self.log(format!("  no reply, resending ({attempt}/5)"));
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(Error::NoReply { cmd })
        }

        pub fn read(&self, cmd: u8, len: u8) -> Result<Vec<u8>, Error> {
            self.transact(cmd, READ, len, &[])
        }

        fn log(&self, s: String) {
            if self.verbose {
                println!("{s}");
            }
        }
    }

    pub struct Status {
        pub wireless: bool,
        pub gen3954: bool,
        pub mouse_pid: u16,
        pub battery: u8, // percent
        pub charging: bool,
        pub standard: bool,
        pub motion_sync: bool,
        pub rate: u8,
    }

    pub fn read_status(d: &Device) -> Result<Status, Error> {
        let wireless = d.is_wireless();
        // Mouse PID and charging status are wireless-only commands; wired PID is the HID PID itself.
        let mouse_pid = if wireless {
            // The receiver answers 0x0F itself. With the mouse off, every mouse command
            // would time out (~3.5 s per refresh, measured), so bail out early.
            if d.read(CMD_WIRELESS, 1)?[5] == 0 {
                return Err(Error::LinkDown);
            }
            let r = d.read(CMD_MOUSE_PID, 2)?;
            u16::from_le_bytes([r[5], r[6]])
        } else {
            d.pid
        };
        let battery = battery_percent(d.read(CMD_BATTERY, 1)?[5]);
        // vcc reads this in wired mode too (a cabled mouse is charging). Like its runAPI,
        // treat an unanswered read as "not charging" instead of failing the whole status.
        let charging = d.read(CMD_CHARGING, 1).is_ok_and(|r| r[5] != 0);
        let (standard, motion_sync) = decode_tracking(&d.read_tracking()?);
        let rate = d.read(CMD_REPORT_RATE, 1)?[5];
        Ok(Status {
            wireless,
            gen3954: d.is_3954(),
            mouse_pid,
            battery,
            charging,
            standard,
            motion_sync,
            rate,
        })
    }

    /// Set report rate index (1=500 .. 5=8000), refusing combos vcc disallows.
    pub fn set_rate(d: &Device, rate: u8) -> Result<(), Error> {
        let (standard, _) = decode_tracking(&d.read_tracking()?);
        let max = max_rate(d.is_3954(), d.is_wireless(), standard);
        if !(1..=max).contains(&rate) {
            return Err(Error::RateTooHigh { max });
        }
        d.transact(CMD_REPORT_RATE, WRITE, 1, &[rate]).map(|_| ())
    }

    /// Switch competitive/standard and Motion Sync. When entering standard mode the report
    /// rate is lowered first, because the firmware accepts out-of-range combos without clamping.
    pub fn set_mode(d: &Device, standard: bool, motion_sync: bool) -> Result<(), Error> {
        // vcc trackingWiredEnabled = [T,F,T,F]: standard modes are wireless-only.
        if standard && !d.is_wireless() {
            return Err(Error::StandardNeedsReceiver);
        }
        if standard {
            let max = max_rate(d.is_3954(), true, true);
            if d.read(CMD_REPORT_RATE, 1)?[5] > max {
                d.transact(CMD_REPORT_RATE, WRITE, 1, &[max])?;
            }
        }
        if d.is_3954() {
            // cls_device_handler.js: [mode, motionSync], mode 0 = standard, 1 = competitive.
            let data = [u8::from(!standard), u8::from(motion_sync)];
            d.transact(CMD_TRACKING, WRITE, 2, &data).map(|_| ())
        } else {
            let index = 1 + u8::from(standard) + 2 * u8::from(motion_sync);
            d.transact(CMD_TRACKING, WRITE, 1, &[index]).map(|_| ())
        }
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn rate_name(i: u8) -> &'static str {
    match i {
        1 => "500Hz",
        2 => "1000Hz",
        3 => "2000Hz",
        4 => "4000Hz",
        5 => "8000Hz",
        _ => "?",
    }
}

/// Tracking reply -> (standard, motion_sync).
/// 1-byte format: index 1-4 (2/4 standard, 3/4 Motion Sync on);
/// 3954 2-byte format: byte5 0 = standard / 1 = competitive, byte6 Motion Sync.
pub fn decode_tracking(r: &[u8]) -> (bool, bool) {
    if r[4] == 2 {
        (r[5] == 0, r[6] != 0)
    } else {
        (r[5] == 2 || r[5] == 4, r[5] >= 3)
    }
}

/// Highest report rate index allowed by vcc for this device state.
/// Wired: 1000Hz. Older mice: standard 1000Hz, competitive 4000Hz.
/// 3954: standard 4000Hz, competitive 8000Hz (cls_device_handler.js).
pub fn max_rate(gen3954: bool, wireless: bool, standard: bool) -> u8 {
    match (wireless, gen3954, standard) {
        (false, _, _) => RATE_1000,
        (true, false, true) => RATE_1000,
        (true, false, false) => RATE_4000,
        (true, true, true) => RATE_4000,
        (true, true, false) => RATE_8000,
    }
}

pub fn tracking_name(r: &[u8]) -> String {
    match (r[4], r[5]) {
        // Newer 3954 format: data length 2, byte5 0=standard 1=gaming (device_factory.js).
        (2, 0) => format!("Standard (2-byte format, byte6=0x{:02X})", r[6]),
        (2, 1) => format!("Competitive (2-byte format, byte6=0x{:02X})", r[6]),
        (_, 1) => "Competitive / Motion Sync off".into(),
        (_, 2) => "Standard / Motion Sync off".into(),
        (_, 3) => "Competitive / Motion Sync on".into(),
        (_, 4) => "Standard / Motion Sync on".into(),
        _ => "?".into(),
    }
}

/// vcc shows "charge now" at or below this percent while discharging (bat.js batLowHint).
pub const LOW_BATTERY: u8 = 10;

/// Low-battery alert latch: fire once when the battery reaches LOW_BATTERY while
/// discharging; re-arm after charging or rising above it. Returns (fire, latched).
pub fn low_battery_alert(latched: bool, percent: u8, charging: bool) -> (bool, bool) {
    if charging || percent > LOW_BATTERY {
        (false, false)
    } else {
        (!latched, true)
    }
}

/// Battery cmd byte5 counts 5% steps, capped at 20 (bat.js).
pub fn battery_percent(raw: u8) -> u8 {
    raw.min(20) * 5
}

pub fn device_name(mouse_pid: u16) -> &'static str {
    match mouse_pid {
        0x1003 => "VAXEE XE Wireless",
        0x1004 => "ZYGEN NP-01S Wireless",
        0x1005 => "VAXEE AX Wireless",
        0x1006 => "ZYGEN NP-01 Wireless",
        0x1007 => "VAXEE XE-S Wireless",
        0x1008 => "VAXEE XE-S-L Wireless",
        0x1009 => "VAXEE x NINJUTSO Sora Wireless",
        0x1010 => "VAXEE E1 Wireless",
        0x1011 => "ZYGEN NP-01S V2 Wireless",
        0x1012 => "VAXEE XE V2 Wireless",
        0x1013 => "ZYGEN NP-01S Ergo Wireless",
        0x1014 => "VAXEE NP-01S V3 Wireless",
        0x1015 => "VAXEE NP-01 Ergo Wireless",
        _ => "VAXEE",
    }
}

fn sensor(mouse_pid: u16) -> Option<u16> {
    match mouse_pid {
        0x1003..=0x1006 => Some(3395),
        0x1007..=0x1013 => Some(3950),
        0x1014 | 0x1015 => Some(3954),
        _ => None,
    }
}

/// Battery label and remaining hours exactly as vcc shows them, e.g. ("20%-30%", "11-15").
/// An empty hours string means "charge now"; None for unknown sensors / modes.
pub fn battery_estimate(
    mouse_pid: u16,
    percent: u8,
    charging: bool,
    standard: bool,
    rate: u8,
) -> Option<(&'static str, &'static str)> {
    let s = sensor(mouse_pid)?;
    // 3954 has no charging table; vcc falls back to the normal one (bat.js).
    let row = bat_table::row(s, charging, standard, rate)
        .or_else(|| bat_table::row(s, false, standard, rate))?;
    Some(row[(percent.min(100) / 5) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_and_eta() {
        // Matches vcc UI for NP-01S, competitive 1K, raw step 5: "20%-30%, 11-15h".
        assert_eq!(battery_percent(5), 25);
        assert_eq!(battery_percent(30), 100);
        let est = |pid, pct, chg, std, rate| battery_estimate(pid, pct, chg, std, rate);
        assert_eq!(
            est(0x1004, 25, false, false, RATE_1000),
            Some(("20%-30%", "11-15"))
        );
        assert_eq!(
            est(0x1004, 100, false, false, RATE_1000),
            Some(("100%", "55"))
        );
        assert_eq!(
            est(0x1004, 25, true, false, RATE_1000),
            Some(("25%", "13.8"))
        );
        assert_eq!(est(0x1004, 10, false, false, RATE_1000), Some(("<10%", "")));
        assert_eq!(est(0x1004, 25, false, true, RATE_4000), None);
        // 3954: own labels, 8K row, standard up to 4K, charging falls back to the normal table.
        assert_eq!(
            est(0x1015, 25, false, false, RATE_1000),
            Some(("20%-30%", "6.5-9.8"))
        );
        assert_eq!(
            est(0x1015, 25, true, false, RATE_1000),
            est(0x1015, 25, false, false, RATE_1000)
        );
        assert!(est(0x1015, 50, false, false, 5).is_some());
        assert!(est(0x1015, 50, false, true, RATE_4000).is_some());
        assert_eq!(est(0x9999, 50, false, false, RATE_1000), None);
    }

    #[test]
    fn low_battery_fires_once_per_discharge() {
        assert_eq!(low_battery_alert(false, 15, false), (false, false));
        assert_eq!(low_battery_alert(false, 10, false), (true, true)); // first time at 10%
        assert_eq!(low_battery_alert(true, 5, false), (false, true)); // already notified
        assert_eq!(low_battery_alert(true, 5, true), (false, false)); // charging re-arms
        assert_eq!(low_battery_alert(false, 5, true), (false, false)); // no alert while charging
        assert_eq!(low_battery_alert(true, 15, false), (false, false)); // recovered re-arms
    }

    #[test]
    fn tracking_and_limits() {
        let reply = |len, a, b| [0x0E, 0xA5, CMD_TRACKING, READ, len, a, b];
        assert_eq!(decode_tracking(&reply(1, 1, 0)), (false, false));
        assert_eq!(decode_tracking(&reply(1, 4, 0)), (true, true));
        assert_eq!(decode_tracking(&reply(2, 0, 1)), (true, true));
        assert_eq!(decode_tracking(&reply(2, 1, 0)), (false, false));
        assert_eq!(max_rate(false, true, true), RATE_1000);
        assert_eq!(max_rate(false, true, false), RATE_4000);
        assert_eq!(max_rate(true, true, true), RATE_4000);
        assert_eq!(max_rate(true, true, false), RATE_8000);
        assert_eq!(max_rate(true, false, false), RATE_1000);
    }
}
