//! Protocol debugging CLI: prints every TX/RX packet.

use mousepulse::*;
use std::env;

fn open() -> Result<Device, String> {
    let mut d = Device::open()?;
    d.verbose = true;
    println!(
        "device PID 0x{:04X}, feature report {} bytes\n",
        d.pid, d.feature_len
    );
    Ok(d)
}

fn info(d: &Device) -> Result<(), String> {
    let mut lines = Vec::new();

    if d.is_wireless() {
        let r = d.read(CMD_MOUSE_PID, 2)?;
        lines.push(format!(
            "mouse PID     : {:04X}",
            u16::from_le_bytes([r[5], r[6]])
        ));
    }
    let r = d.read(CMD_FIRMWARE, 2)?;
    lines.push(format!(
        "firmware      : {:X}",
        u16::from_le_bytes([r[5], r[6]])
    ));
    let r = d.read(CMD_BATTERY, 1)?;
    // bytes 6-7 look like cell voltage in mV (LE), byte8 unknown.
    lines.push(format!(
        "battery       : {}%  (raw steps {}, bytes6-7 = {} mV?, byte8 = 0x{:02X})",
        battery_percent(r[5]),
        r[5],
        u16::from_le_bytes([r[6], r[7]]),
        r[8]
    ));
    let r = d.read_tracking()?;
    lines.push(format!(
        "tracking      : {} (raw len={} val={})",
        tracking_name(&r),
        r[4],
        r[5]
    ));
    let r = d.read(CMD_REPORT_RATE, 1)?;
    lines.push(format!(
        "report rate   : {} (raw {})",
        rate_name(r[5]),
        r[5]
    ));
    // Read both in wired mode too; vcc does, and a failure is informative here.
    for (cmd, name) in [
        (CMD_CHARGING, "charging     "),
        (CMD_WIRELESS, "wireless link"),
    ] {
        match d.read(cmd, 1) {
            Ok(r) => lines.push(format!("{name} : raw {}", r[5])),
            Err(e) => lines.push(format!("{name} : {e}")),
        }
    }
    lines.push(format!(
        "connection    : {}",
        if d.is_wireless() { "receiver" } else { "cable" }
    ));

    println!("\n{}", lines.join("\n"));
    Ok(())
}

fn read_back(d: &Device) -> Result<(), String> {
    println!("\n--- read back ---");
    let t = d.read_tracking()?;
    let r = d.read(CMD_REPORT_RATE, 1)?;
    println!(
        "\ntracking    : {}\nreport rate : {}",
        tracking_name(&t),
        rate_name(r[5])
    );
    Ok(())
}

fn parse_hex(s: &str) -> Result<u8, String> {
    u8::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| format!("bad hex byte: {s}"))
}

/// Validate a 1..=max argument before touching the device.
fn parse_arg(arg: Option<&String>, max: u8, name: &str) -> Result<u8, String> {
    arg.and_then(|s| s.parse().ok())
        .filter(|v| (1..=max).contains(v))
        .ok_or_else(|| format!("usage: {name} <1-{max}>"))
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("info") => info(&open()?),
        Some("probe") => {
            for d in Device::open_all() {
                let d = match d {
                    Ok(d) => d,
                    Err(e) => {
                        println!("{e}");
                        continue;
                    }
                };
                match read_status(&d).map(|s| (d.pid, s)) {
                    Ok((pid, s)) => println!(
                        "PID {pid:04X} ({})  mouse {:04X}  battery {}%  charging {}  {} / MS {}  {}",
                        if s.wireless { "receiver" } else { "cable" },
                        s.mouse_pid,
                        s.battery,
                        s.charging,
                        if s.standard { "standard" } else { "competitive" },
                        if s.motion_sync { "on" } else { "off" },
                        rate_name(s.rate),
                    ),
                    Err(e) => println!("PID {:04X}: {e}", d.pid),
                }
            }
            Ok(())
        }
        Some("list") => {
            let all = enumerate();
            if all.is_empty() {
                println!("no HID collections with VID 0x{VID:04X}");
            }
            for d in all {
                println!(
                    "PID {:04X}  usage {:04X}:{:04X}  feature {:>3}B  {}",
                    d.pid,
                    d.usage_page,
                    d.usage,
                    d.feature_len,
                    String::from_utf16_lossy(&d.path[..d.path.len() - 1])
                );
            }
            Ok(())
        }
        Some("mode") => {
            let m = parse_arg(args.get(1), 4, "mode")?;
            let d = open()?;
            set_mode(&d, matches!(m, 2 | 4), m >= 3)?;
            read_back(&d)
        }
        Some("rate") => {
            let r = parse_arg(args.get(1), 5, "rate")?;
            let d = open()?;
            set_rate(&d, r)?;
            read_back(&d)
        }
        Some("raw") if args.len() >= 4 => {
            let b: Vec<u8> = args[1..]
                .iter()
                .map(|s| parse_hex(s))
                .collect::<Result<_, _>>()?;
            open()?.transact(b[0], b[1], b[2], &b[3..])?;
            Ok(())
        }
        _ => Err(HELP.into()),
    }
}

const HELP: &str = "\
usage: mousepulse-cli [command]
  info              read PID, firmware, battery, mode, report rate (default)
  list              list all HID collections with VID 0x3057
  probe             read status from every VAXEE device (receiver and cable)
  mode <1-4>        1=Competitive/MS off  2=Standard/MS off
                    3=Competitive/MS on   4=Standard/MS on
  rate <1-5>        1=500Hz 2=1000Hz 3=2000Hz 4=4000Hz 5=8000Hz (3954)
  raw <cmd> <rw> <len> [data..]   send a raw command (hex), e.g. raw 0B 01 01";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
