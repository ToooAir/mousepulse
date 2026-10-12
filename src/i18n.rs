//! Tray UI strings in Traditional Chinese, Simplified Chinese and English,
//! picked from the Windows display language. `{name}`-style placeholders are
//! filled with `str::replace` by the caller.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lang {
    ZhHant,
    ZhHans,
    En,
}

/// UI language for a Windows LANGID (GetUserDefaultUILanguage): Simplified
/// Chinese for mainland China / Singapore, Traditional for other Chinese
/// locales, English for everything else.
pub fn lang_from_langid(id: u16) -> Lang {
    const LANG_CHINESE: u16 = 0x04;
    if id & 0x3FF != LANG_CHINESE {
        return Lang::En;
    }
    match id {
        // zh-CN, zh-SG, and the neutral zh-Hans / zh LANGIDs
        0x0804 | 0x1004 | 0x0004 | 0x7804 => Lang::ZhHans,
        _ => Lang::ZhHant,
    }
}

/// `--lang=<value>` override for testing other languages without changing Windows.
pub fn lang_from_arg(value: &str) -> Option<Lang> {
    match value.to_ascii_lowercase().as_str() {
        "en" => Some(Lang::En),
        "zh-hant" | "zh-tw" => Some(Lang::ZhHant),
        "zh-hans" | "zh-cn" => Some(Lang::ZhHans),
        _ => None,
    }
}

pub struct Strings {
    // Menu
    pub competitive_item: &'static str,
    pub standard_item: &'static str,
    pub motion_sync_item: &'static str,
    pub report_rate_item: &'static str,
    pub autostart_item: &'static str,
    pub refresh_item: &'static str,
    pub exit_item: &'static str,
    // Tooltip
    pub battery_line: &'static str, // {label} {charging} {eta}
    pub charging_suffix: &'static str,
    pub eta_hours: &'static str, // {h}
    pub charge_now: &'static str,
    pub mode_line: &'static str, // {mode} {ms}
    pub competitive: &'static str,
    pub standard: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub disconnected: &'static str,
    // Notifications and dialogs
    pub low_battery_title: &'static str,
    pub low_battery_body: &'static str, // {name}
    pub already_running: &'static str,
    pub autostart_failed: &'static str, // {code}
    // Errors
    pub err_not_found: &'static str,
    pub err_open: &'static str, // {code}
    pub err_no_reply: &'static str,
    pub err_rate_too_high: &'static str, // {max}
    pub err_standard_cable: &'static str,
    pub err_internal: &'static str, // {detail}
}

pub const ZH_HANT: Strings = Strings {
    competitive_item: "競技模式",
    standard_item: "標準模式",
    motion_sync_item: "Motion Sync",
    report_rate_item: "回報率",
    autostart_item: "開機自動啟動",
    refresh_item: "重新整理",
    exit_item: "結束",
    battery_line: "電量 {label}{charging}  {eta}",
    charging_suffix: "（充電中）",
    eta_hours: "約 {h} 小時",
    charge_now: "請立即充電",
    mode_line: "{mode}模式 (Motion Sync {ms})",
    competitive: "競技",
    standard: "標準",
    on: "開",
    off: "關",
    disconnected: "VAXEE 滑鼠未連線",
    low_battery_title: "VAXEE 電量過低",
    low_battery_body: "{name} 電量即將耗盡，請立即充電。",
    already_running: "MousePulse 已在執行中，請查看系統匣。",
    autostart_failed: "無法修改開機自動啟動設定（錯誤碼 {code}）",
    err_not_found: "找不到 VAXEE 滑鼠",
    err_open: "無法開啟裝置（錯誤碼 {code}）",
    err_no_reply: "滑鼠沒有回應，請確認滑鼠已開機並在接收範圍內",
    err_rate_too_high: "目前模式最高只支援 {max}",
    err_standard_cable: "標準模式只能透過接收器使用",
    err_internal: "內部錯誤：{detail}",
};

pub const ZH_HANS: Strings = Strings {
    competitive_item: "竞技模式",
    standard_item: "标准模式",
    motion_sync_item: "Motion Sync",
    report_rate_item: "回报率",
    autostart_item: "开机自动启动",
    refresh_item: "刷新",
    exit_item: "退出",
    battery_line: "电量 {label}{charging}  {eta}",
    charging_suffix: "（充电中）",
    eta_hours: "约 {h} 小时",
    charge_now: "请立即充电",
    mode_line: "{mode}模式 (Motion Sync {ms})",
    competitive: "竞技",
    standard: "标准",
    on: "开",
    off: "关",
    disconnected: "VAXEE 鼠标未连接",
    low_battery_title: "VAXEE 电量过低",
    low_battery_body: "{name} 电量即将耗尽，请立即充电。",
    already_running: "MousePulse 已在运行，请查看系统托盘。",
    autostart_failed: "无法修改开机自动启动设置（错误码 {code}）",
    err_not_found: "找不到 VAXEE 鼠标",
    err_open: "无法打开设备（错误码 {code}）",
    err_no_reply: "鼠标没有响应，请确认鼠标已开机并在接收范围内",
    err_rate_too_high: "当前模式最高只支持 {max}",
    err_standard_cable: "标准模式只能通过接收器使用",
    err_internal: "内部错误：{detail}",
};

pub const EN: Strings = Strings {
    competitive_item: "Competitive mode",
    standard_item: "Standard mode",
    motion_sync_item: "Motion Sync",
    report_rate_item: "Report rate",
    autostart_item: "Start with Windows",
    refresh_item: "Refresh",
    exit_item: "Exit",
    battery_line: "Battery {label}{charging}  {eta}",
    charging_suffix: " (charging)",
    eta_hours: "~{h} h left",
    charge_now: "Charge now",
    mode_line: "{mode} mode (Motion Sync {ms})",
    competitive: "Competitive",
    standard: "Standard",
    on: "on",
    off: "off",
    disconnected: "VAXEE mouse not connected",
    low_battery_title: "VAXEE battery low",
    low_battery_body: "{name} is almost out of battery. Charge it now.",
    already_running: "MousePulse is already running. Look for it in the system tray.",
    autostart_failed: "Could not change the startup setting (error {code}).",
    err_not_found: "No VAXEE mouse found",
    err_open: "Could not open the device (error {code})",
    err_no_reply: "The mouse is not responding. Make sure it is on and in range of the receiver.",
    err_rate_too_high: "This mode supports up to {max}",
    err_standard_cable: "Standard mode is only available over the wireless receiver",
    err_internal: "Internal error: {detail}",
};

impl Lang {
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::ZhHant => &ZH_HANT,
            Lang::ZhHans => &ZH_HANS,
            Lang::En => &EN,
        }
    }
}

impl Strings {
    /// User-facing text for a protocol error.
    pub fn error(&self, e: &Error) -> String {
        match e {
            Error::NotFound => self.err_not_found.into(),
            Error::Open { code, .. } => self.err_open.replace("{code}", &code.to_string()),
            Error::NoReply { .. } | Error::LinkDown => self.err_no_reply.into(),
            Error::RateTooHigh { max } => self
                .err_rate_too_high
                .replace("{max}", crate::rate_name(*max)),
            Error::StandardNeedsReceiver => self.err_standard_cable.into(),
            Error::BadReportLength { .. } | Error::DataTooLong => {
                self.err_internal.replace("{detail}", &e.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_language_from_langid() {
        assert_eq!(lang_from_langid(0x0404), Lang::ZhHant); // zh-TW
        assert_eq!(lang_from_langid(0x0C04), Lang::ZhHant); // zh-HK
        assert_eq!(lang_from_langid(0x1404), Lang::ZhHant); // zh-MO
        assert_eq!(lang_from_langid(0x0804), Lang::ZhHans); // zh-CN
        assert_eq!(lang_from_langid(0x1004), Lang::ZhHans); // zh-SG
        assert_eq!(lang_from_langid(0x0409), Lang::En); // en-US
        assert_eq!(lang_from_langid(0x0411), Lang::En); // ja-JP
        assert_eq!(lang_from_arg("EN"), Some(Lang::En));
        assert_eq!(lang_from_arg("zh-CN"), Some(Lang::ZhHans));
        assert_eq!(lang_from_arg("zh-hant"), Some(Lang::ZhHant));
        assert_eq!(lang_from_arg("fr"), None);
    }

    #[test]
    fn every_language_keeps_its_placeholders() {
        for s in [&ZH_HANT, &ZH_HANS, &EN] {
            for (text, holes) in [
                (s.battery_line, &["{label}", "{charging}", "{eta}"][..]),
                (s.eta_hours, &["{h}"]),
                (s.mode_line, &["{mode}", "{ms}"]),
                (s.low_battery_body, &["{name}"]),
                (s.autostart_failed, &["{code}"]),
                (s.err_open, &["{code}"]),
                (s.err_rate_too_high, &["{max}"]),
                (s.err_internal, &["{detail}"]),
            ] {
                for h in holes {
                    assert!(text.contains(h), "{text:?} is missing {h}");
                }
            }
        }
    }

    #[test]
    fn translates_errors() {
        let e = Error::RateTooHigh {
            max: crate::RATE_1000,
        };
        assert_eq!(EN.error(&e), "This mode supports up to 1000Hz");
        assert_eq!(ZH_HANT.error(&e), "目前模式最高只支援 1000Hz");
    }
}
