# MousePulse

English | [繁體中文](README.zh-TW.md)

**An unofficial battery tray for VAXEE mice.** A tiny Windows tray app for VAXEE wireless mice: battery level at a glance,
remaining runtime, and one-click switching between Competitive and Standard mode.

It talks to the mouse directly over HID, using the same protocol as the official
web driver ([VAXEE Control Center](https://vcc.vaxee.co/)), so you don't need to
open a browser just to check the battery.

> **Unofficial.** This project is not affiliated with or endorsed by VAXEE.
> Product names are used only to identify compatible hardware.

## Features

- **Battery icon in the tray.** Fill level and color follow the battery
  (green ≥ 50%, orange 20–49%, red < 20%). A bolt replaces the bar while charging,
  and a grey empty battery means no mouse was found. The outline follows the
  light/dark taskbar theme.
- **Tooltip** with the model, battery, remaining runtime, current mode and report
  rate. Battery labels and runtimes are taken verbatim from VAXEE Control Center,
  so both show the same values (for example `50%-70%`, `27.5-37 h`).
- **Right-click menu**
  - Competitive mode (also sets 4000 Hz over the receiver)
  - Standard mode (also sets 1000 Hz)
  - Motion Sync on/off, keeping the current mode
  - Report rate submenu; rates the current mode doesn't allow are greyed out
  - Start with Windows
  - Refresh / Exit
- **Low-battery notification**, once per discharge, at 10% (the same "charge now"
  threshold as the official driver).
- Works over the receiver or the charging cable. When both are plugged in, the
  cabled connection is used, because the receiver loses its link to the mouse.
- English, Traditional Chinese and Simplified Chinese UI, following the Windows
  display language.
- About 330 KB, a single exe with no runtime or installer. Only a single instance
  runs at a time.

## Requirements

- Windows 10 or 11 (x64)
- A supported mouse (see below), connected through its receiver or a USB cable

## Usage

1. Download or build `mousepulse.exe` and put it somewhere permanent, for example
   `C:\Tools\`. Turning on *Start with Windows* stores this path, so avoid
   `Downloads`.
2. Run it. The battery icon appears in the notification area; if you can't see it,
   look under the `^` overflow.
3. Hover for details, right-click for the menu, left-click to refresh now.
   Otherwise the tool refreshes every 60 seconds.

Close VAXEE Control Center while the tray app is running, because commands sent
from both at the same time can interfere with each other.

### Mode and report rate rules

The tool enforces the same limits as VAXEE Control Center:

| Connection / mouse          | Standard mode | Competitive mode |
|-----------------------------|---------------|------------------|
| Cable (any model)           | not available | up to 1000 Hz    |
| Receiver, 3395 / 3950 mice  | up to 1000 Hz | up to 4000 Hz    |
| Receiver, 3954 mice         | up to 4000 Hz | up to 8000 Hz    |

The firmware itself accepts out-of-range combinations, such as Standard mode at
4000 Hz, so the tool lowers the report rate before it enters Standard mode.

## Supported devices

| Model                           | Mouse PID | Sensor | Status |
|---------------------------------|-----------|--------|--------|
| NP-01S                          | 1004      | 3395   | Tested (receiver 2001 and cable) |
| XE / AX / NP-01                 | 1003, 1005, 1006 | 3395 | Same protocol as above, untested |
| XE-S / XE-S-L / Sora / E1 / XE V2 / NP-01S V2 / NP-01S Ergo | 1007–1013 | 3950 | Untested |
| NP-01S V3 / NP-01 Ergo          | 1014, 1015 | 3954  | Implemented from the official driver's code, untested |

Receivers: 1001, 1002, 2001 (4K/8K), 2002 (8K). If your mouse works (or doesn't),
an issue with the output of `mousepulse-cli probe` is very welcome.

## Command-line tool

`mousepulse-cli.exe` is a debugging companion that prints every packet it sends and
receives.

```
mousepulse-cli                 read PID, firmware, battery, mode, report rate
mousepulse-cli list            list all HID collections with VID 0x3057
mousepulse-cli probe           read status from every VAXEE device (receiver and cable)
mousepulse-cli mode <1-4>      1=Competitive/MS off  2=Standard/MS off
                               3=Competitive/MS on   4=Standard/MS on
mousepulse-cli rate <1-5>      1=500Hz 2=1000Hz 3=2000Hz 4=4000Hz 5=8000Hz (3954)
mousepulse-cli raw <cmd> <rw> <len> [data..]   send a raw command (hex), e.g. raw 0B 01 01
```

`mousepulse.exe` also accepts two options for testing:

- `--test-low-battery` shows the low-battery notification right away, for checking
  notification settings.
- `--lang=en`, `--lang=zh-hant` or `--lang=zh-hans` overrides the display language.

## Building

The project targets `x86_64-pc-windows-gnu` and can be cross-compiled from macOS
or Linux.

```sh
rustup target add x86_64-pc-windows-gnu
brew install mingw-w64            # or: apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/mousepulse.exe, mousepulse-cli.exe
```

`build.rs` embeds the app icon and version info with mingw-w64's `windres`. On
other Windows targets (MSVC) the build still succeeds but prints a warning, and
the exe has no icon.

The only dependency is [`windows-sys`](https://crates.io/crates/windows-sys);
everything else is plain Win32 (`hid.dll`, `setupapi`, `Shell_NotifyIconW`).

### Tests

The protocol helpers, battery tables and icon rasterizer are portable, so the
tests run on any host:

```sh
cargo test --lib
```

## How it works

The mouse exposes a vendor HID collection (VID `0x3057`, usage page `0xFF05`).
Commands are 64-byte feature reports with ID `0x0E`:

```
request : 0E A5 <cmd> <rw: 01 read / 02 write> <len> <data...>
response: 0E A5 <cmd> <01 data / 03 ack> <len> <data...>   (data starts at byte 5)
```

| cmd  | Meaning |
|------|---------|
| 0x01 | Firmware version |
| 0x07 | Report rate (1=500 … 4=4000, 5=8000) |
| 0x08 | Mode: 1 byte `1..4` (Competitive/Standard × Motion Sync); 3954 uses 2 bytes `[mode, motionSync]` |
| 0x0A | Mouse PID (receiver only) |
| 0x0B | Battery, in 5% steps (0–20) |
| 0x0F | Wireless link status |
| 0x10 | Charging status (works over the cable too) |

Writes can take more than 100 ms to acknowledge, so the tool polls the reply
instead of resending the command.

## Project layout

```
src/lib.rs           protocol, device access (Win32 parts behind cfg(windows))
src/bat_table.rs     battery labels / runtimes, generated from the official driver
src/icon.rs          tray battery icon rasterizer
src/main.rs          tray app (mousepulse.exe)
src/bin/mousepulse-cli.rs debugging CLI (mousepulse-cli.exe)
build.rs             embeds assets/app.ico and version info
tools/gen_bat_table.py  regenerate src/bat_table.rs from bat_paramter.js
tools/gen_icon.py       regenerate assets/app.ico
```

To refresh the battery tables after VAXEE Control Center updates, save
`https://vcc.vaxee.co/assets/js/vcc-mouse/paramter/bat_paramter.js` from your
browser, then run:

```sh
python3 tools/gen_bat_table.py path/to/bat_paramter.js > src/bat_table.rs
cargo fmt
```

## Limitations

- Runtime estimates come from VAXEE's own tables, not from measuring your usage.
  Below 70% they are ranges, which reflects how coarse the battery reading is.
- The tray app reads the mouse on the UI thread, so the menu can pause for about
  half a second during a refresh.
- Plugging or unplugging the cable is picked up on the next refresh (at most
  60 s, or left-click the icon).
- User profiles (3954 receivers), DPI, debounce and button remapping are not
  exposed; use VAXEE Control Center for those.

## License

[MIT](LICENSE)
