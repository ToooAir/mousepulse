//! Embed assets/app.ico and version info into the Windows executables via windres
//! (ships with mingw-w64). No build dependencies needed.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        println!("cargo:warning=app icon is only embedded for *-windows-gnu targets");
        return;
    }

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/app.ico");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = version.replace('.', ",") + ",0";
    let rc = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "VAXEE Battery Tray"
      VALUE "ProductName", "vaxee-tool"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico.display().to_string().replace('\\', "/"),
    );
    let rc_path = out.join("app.rc");
    let obj = out.join("app_res.o");
    fs::write(&rc_path, rc).unwrap();

    // Cross builds use the target-prefixed binary; native Windows GNU toolchains ship plain `windres`.
    let windres = if env::var("HOST") == env::var("TARGET") {
        "windres".to_string()
    } else {
        format!("{}-w64-mingw32-windres", env::var("CARGO_CFG_TARGET_ARCH").unwrap())
    };
    let status = Command::new(&windres)
        .args(["--output-format=coff", "--input"])
        .arg(&rc_path)
        .arg("--output")
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {windres}: {e}"));
    assert!(status.success(), "{windres} failed");
    println!("cargo:rustc-link-arg-bins={}", obj.display());
}
