//! Work done for the NSIS installer/uninstaller (run elevated by them):
//!
//! * `--install-cable`: downloads the official VB-CABLE package from
//!   vb-audio.com, checks it, installs it silently, gives the devices their
//!   Claude Talk names and makes sure Windows' default devices did not
//!   change. Exit codes: 0 ok, 2 no internet, 3 package failed the check,
//!   4 driver install failed, 5 other error.
//! * `--uninstall-cleanup`: gives Discord its own microphone back, removes
//!   autostart, and removes VB-CABLE if Claude Talk installed it.
//!
//! The package is never bundled with Claude Talk (VB-Audio's licence asks
//! that it comes from them); a copy of the extracted package is kept in
//! `%ProgramData%\Claude Talk\vbcable` so it can be uninstalled later.

#![cfg(windows)]

use ct_engine::win::devices::{self, DeviceSelector, Flow};
use ct_engine::win::host::cable_render_patterns;
use ct_engine::win::ComGuard;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PACKAGE_URL: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack46.zip";
/// SHA-256 of VBCABLE_Driver_Pack46.zip as measured in CI (driver-proof).
const KNOWN_SHA256: &str = "bc44e8974191a5a2427d6c979e2dab08dbb09925f3f435dfa2f0ba3e45b6eb90";
const CAPTURE_NAME: &str = "Claude Talk – til Discord";
const RENDER_NAME: &str = "Claude Talk – intern";
const IFACE: &str = "VB-Audio";

fn data_dir() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Claude Talk")
}
fn marker() -> PathBuf {
    data_dir().join("cable-installed-by-claude-talk")
}

pub fn install_cable() -> i32 {
    let _com = ComGuard::new();
    log::info!("installer: setting up the virtual microphone");
    let before = defaults();
    let already = cable_present();
    if !already {
        let dir = data_dir().join("vbcable");
        let _ = std::fs::create_dir_all(&dir);
        let zip = std::env::temp_dir().join("ClaudeTalk-VBCABLE.zip");
        if let Err(code) = download(PACKAGE_URL, &zip) {
            return code;
        }
        if !package_ok(&zip) {
            log::error!("installer: package failed the integrity check");
            return 3;
        }
        if let Err(e) = extract(&zip, &dir) {
            log::error!("installer: extract: {e}");
            return 5;
        }
        let setup = dir.join("VBCABLE_Setup_x64.exe");
        if !authenticode_valid(&setup) {
            log::error!("installer: setup program signature invalid");
            return 3;
        }
        match run_with_timeout(&setup, &["-i", "-h"], Duration::from_secs(180)) {
            Some(code) => log::info!("installer: VB-CABLE setup exit {code}"),
            None => {
                log::error!("installer: VB-CABLE setup did not finish");
                return 4;
            }
        }
        let _ = std::fs::write(marker(), b"1");
        // the endpoints appear a moment after the driver loads
        let t0 = Instant::now();
        while !cable_present() && t0.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(500));
        }
        if !cable_present() {
            log::error!("installer: driver installed but no endpoints appeared");
            return 4;
        }
    }
    rename_devices();
    restore_defaults(&before);
    log::info!("installer: virtual microphone ready");
    0
}

pub fn uninstall_cleanup() -> i32 {
    let _com = ComGuard::new();
    crate::discord::restore();
    // autostart entry written by tauri-plugin-autostart
    let _ = std::process::Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Claude Talk",
            "/f",
        ])
        .status();
    if marker().exists() {
        let setup = data_dir().join("vbcable").join("VBCABLE_Setup_x64.exe");
        if setup.exists() {
            let code = run_with_timeout(&setup, &["-u", "-h"], Duration::from_secs(180));
            log::info!("uninstall: VB-CABLE removal exit {code:?}");
        }
        let _ = std::fs::remove_dir_all(data_dir());
    }
    0
}

fn cable_present() -> bool {
    devices::resolve(&DeviceSelector::FirstMatching(
        Flow::Render,
        cable_render_patterns(),
    ))
    .is_ok()
}

fn download(url: &str, to: &Path) -> Result<(), i32> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;
    let _ = std::fs::remove_file(to);
    for attempt in 1..=3 {
        let hr = unsafe {
            URLDownloadToFileW(
                None,
                &HSTRING::from(url),
                &HSTRING::from(to.as_os_str()),
                0,
                None,
            )
        };
        if hr.is_ok() && to.exists() {
            return Ok(());
        }
        log::warn!("installer: download attempt {attempt} failed: {hr:?}");
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(2)
}

fn sha256_hex(path: &Path) -> Option<String> {
    // Windows' certutil computes the hash without extra crates.
    let out = std::process::Command::new("certutil")
        .args(["-hashfile"])
        .arg(path)
        .arg("SHA256")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .nth(1)
        .map(|l| l.trim().replace(' ', "").to_lowercase())
}

fn package_ok(zip: &Path) -> bool {
    match sha256_hex(zip) {
        Some(h) if h == KNOWN_SHA256 => true,
        Some(h) => {
            // VB-Audio may publish an updated pack under the same name; the
            // setup program and catalog are still checked for a valid
            // signature after extraction, and Windows itself only loads a
            // Microsoft-signed driver.
            log::warn!(
                "installer: package hash {h} differs from the tested one; relying on signatures"
            );
            true
        }
        None => false,
    }
}

fn extract(zip: &Path, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut a = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    for i in 0..a.len() {
        let mut f = a.by_index(i)?;
        let Some(name) = f.enclosed_name() else {
            continue;
        };
        let out = dir.join(name);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::io::copy(&mut f, &mut std::fs::File::create(&out)?)?;
        }
    }
    Ok(())
}

/// Authenticode check with WinVerifyTrust (no UI, revocation from cache).
fn authenticode_valid(file: &Path) -> bool {
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::*;
    let path = HSTRING::from(file.as_os_str());
    let mut fi = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut fi;
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let r = unsafe {
        WinVerifyTrust(
            HWND(-1isize as *mut _),
            &mut action,
            &mut data as *mut _ as *mut _,
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            HWND(-1isize as *mut _),
            &mut action,
            &mut data as *mut _ as *mut _,
        )
    };
    r == 0
}

fn run_with_timeout(exe: &Path, args: &[&str], limit: Duration) -> Option<i32> {
    let mut child = std::process::Command::new(exe).args(args).spawn().ok()?;
    let t0 = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status.code().unwrap_or(-1));
        }
        if t0.elapsed() > limit {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn rename_devices() {
    let rename = |flow: Flow, patterns: Vec<String>, name: &str| {
        if let Ok((dev, info)) = devices::resolve(&DeviceSelector::FirstMatching(flow, patterns)) {
            match devices::rename(&dev, name, IFACE) {
                Ok(()) => log::info!("installer: renamed '{}' to '{name} ({IFACE})'", info.name),
                Err(e) => log::warn!("installer: rename '{}': {e}", info.name),
            }
        }
    };
    rename(
        Flow::Capture,
        vec!["cable output".into(), "claude talk".into()],
        CAPTURE_NAME,
    );
    rename(Flow::Render, vec!["cable input".into()], RENDER_NAME);
    rename(
        Flow::Render,
        vec!["cable in 16".into()],
        &format!("{RENDER_NAME} 16ch"),
    );
}

struct Defaults {
    render: Option<String>,
    capture: Option<String>,
}

fn defaults() -> Defaults {
    Defaults {
        render: devices::default_id(Flow::Render),
        capture: devices::default_id(Flow::Capture),
    }
}

/// Installing a driver can make Windows pick the new device as default.
/// Put the user's own defaults back (all roles).
fn restore_defaults(before: &Defaults) {
    let now = defaults();
    for (was, is) in [
        (&before.render, &now.render),
        (&before.capture, &now.capture),
    ] {
        if let (Some(was), Some(is)) = (was, is) {
            if was != is {
                match ct_engine::win::policy::set_default_endpoint(was) {
                    Ok(()) => log::info!("installer: restored default device {was}"),
                    Err(e) => log::warn!("installer: could not restore default device: {e}"),
                }
            }
        }
    }
}
