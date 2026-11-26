//! Work done for the NSIS installer/uninstaller (run elevated by them):
//!
//! * `--install-cable`: downloads the official VB-CABLE package from
//!   vb-audio.com, checks it, installs it silently, gives the devices their
//!   Claude Talk names and makes sure Windows' default devices did not
//!   change. Exit codes: 0 ok, 2 no internet, 3 package failed the check,
//!   4 driver install failed, 5 other error, 6 no driver for this PC
//!   (Windows on ARM and the package has no ARM64 driver).
//! * `--uninstall-cleanup`: gives Discord its own microphone back, removes
//!   autostart, and removes VB-CABLE if Claude Talk installed it.
//!
//! The package is never bundled with Claude Talk (VB-Audio's licence asks
//! that it comes from them); a copy of the extracted package is kept in
//! `Program Files\Claude Talk\vbcable` (admin-only) so it can be uninstalled
//! later.

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

/// Windows' Program Files folder, asked from Windows itself (environment
/// variables can be set by whoever starts the program).
pub fn program_files() -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        match SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None) {
            Ok(p) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                if !s.is_empty() {
                    return PathBuf::from(s);
                }
                PathBuf::from(r"C:\Program Files")
            }
            Err(_) => PathBuf::from(r"C:\Program Files"),
        }
    }
}

/// Kept under Program Files: only administrators can write there, so nobody
/// can plant or swap the files the elevated installer and uninstaller run.
fn data_dir() -> PathBuf {
    program_files().join("Claude Talk").join("vbcable")
}

/// Log folder for the elevated modes: not the user's %APPDATA% (an
/// elevated program must not write where the user can redirect it).
pub fn elevated_log_dir() -> PathBuf {
    program_files().join("Claude Talk").join("logs")
}

/// Windows on ARM (the program itself runs emulated as x64 there).
fn is_arm64() -> bool {
    use windows::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_ARM64;
    use windows::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native) = (Default::default(), Default::default());
    unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, Some(&mut native)) }.is_ok()
        && native == IMAGE_FILE_MACHINE_ARM64
}

/// The ARM64 setup program and driver catalog in an extracted package.
fn find_arm64(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    let named = |ext: &str| {
        files.iter().find(|p| {
            let n = p
                .file_name()
                .map(|f| f.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            n.contains("arm64") && n.ends_with(ext)
        })
    };
    Some((named(".exe")?.clone(), named(".cat")?.clone()))
}

/// Who signs VB-Audio's own setup program (Vincent Burel's VB-Audio).
fn vb_audio_signer(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("burel") || n.contains("vb-audio")
}
/// Written when Claude Talk installed VB-CABLE; holds the SHA-256 of the
/// setup program, checked again before it is run to uninstall.
fn marker() -> PathBuf {
    data_dir().join("installed-by-claude-talk")
}
const SETUP_EXE: &str = "VBCABLE_Setup_x64.exe";
const CATALOG: &str = "vbaudio_cable64_win10.cat";
const WHCP: &str = "Microsoft Windows Hardware Compatibility Publisher";

pub fn install_cable() -> i32 {
    let _com = ComGuard::new();
    log::info!("installer: setting up the virtual microphone");
    let before = defaults();
    if cable_present() {
        // Either ours from an earlier install (already renamed), or a VB-CABLE
        // the user had before: that one keeps its names – we use it as it is.
        log::info!("installer: VB-CABLE is already installed; nothing to install");
        if marker().exists() {
            rename_devices();
        }
        restore_defaults(&before);
        return 0;
    }
    let dir = data_dir();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::error!("installer: {}: {e}", dir.display());
        return 5;
    }
    let zip = dir.join("VBCABLE_Driver_Pack.zip");
    if let Err(code) = download(PACKAGE_URL, &zip) {
        return code;
    }
    let hash_ok = sha256_hex(&zip).is_some_and(|h| h == KNOWN_SHA256);
    log::info!(
        "installer: package hash {}",
        if hash_ok {
            "is the tested one"
        } else {
            "is new"
        }
    );
    if let Err(e) = extract(&zip, &dir) {
        log::error!("installer: extract: {e}");
        return 5;
    }
    let _ = std::fs::remove_file(&zip);
    // Windows on ARM needs VB-Audio's ARM64 setup and driver; the x64 driver
    // cannot load there.
    let (setup, catalog) = if is_arm64() {
        match find_arm64(&dir) {
            Some(found) => found,
            None => {
                log::error!("installer: this PC is ARM64 and the package has no ARM64 driver");
                let _ = std::fs::remove_dir_all(&dir);
                return 6;
            }
        }
    } else {
        (dir.join(SETUP_EXE), dir.join(CATALOG))
    };
    // The tested package (exact hash), or a newer one whose driver catalog
    // is signed by Microsoft's hardware program and whose setup program is
    // signed by VB-Audio; signatures checked with revocation (we are online:
    // the package was just downloaded).
    let catalog_ok = signer_name(&catalog, true).is_some_and(|n| n.contains(WHCP));
    // the tested package is proven by its hash: no online revocation check
    // needed (a firewall may block the revocation servers)
    let setup_signer = signer_name(&setup, !hash_ok);
    let setup_ok = match &setup_signer {
        Some(_) if hash_ok => true,
        Some(n) => vb_audio_signer(n),
        None => false,
    };
    if !(hash_ok || catalog_ok) || !setup_ok {
        log::error!("installer: package failed the checks (hash {hash_ok}, catalog {catalog_ok})");
        let _ = std::fs::remove_dir_all(&dir);
        return 3;
    }
    let setup_hash = sha256_hex(&setup).unwrap_or_default();
    match run_with_timeout(&setup, &["-i", "-h"], Duration::from_secs(180)) {
        Some(code) => log::info!("installer: VB-CABLE setup exit {code}"),
        None => {
            log::error!("installer: VB-CABLE setup did not finish");
            return 4;
        }
    }
    // the endpoints appear a moment after the driver loads
    let t0 = Instant::now();
    while !cable_present() && t0.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(500));
    }
    if !cable_present() {
        log::error!("installer: driver installed but no endpoints appeared");
        return 4;
    }
    // Only now is it ours: the marker lets the uninstaller remove it, and
    // only with this very setup program.
    let _ = std::fs::write(marker(), setup_hash);
    rename_devices();
    restore_defaults(&before);
    log::info!("installer: virtual microphone ready");
    0
}

pub fn uninstall_cleanup() -> i32 {
    let _com = ComGuard::new();
    // Claude Talk itself is still running in the tray: stop it first, so its
    // streams don't hold the cable and the Claude window's files while they
    // are removed (this cleanup gives Discord its microphone back itself)
    stop_other_instances();
    crate::discord::restore();
    remove_autostart();
    // VB-CABLE is removed only if Claude Talk installed it, and only with
    // the very setup program that installed it.
    if let Ok(expected) = std::fs::read_to_string(marker()) {
        let setup = if is_arm64() {
            find_arm64(&data_dir()).map(|(s, _)| s).unwrap_or_default()
        } else {
            data_dir().join(SETUP_EXE)
        };
        let same =
            !expected.trim().is_empty() && sha256_hex(&setup).is_some_and(|h| h == expected.trim());
        // no revocation check here: uninstalling must work offline, and the
        // exact hash is the real check
        if same && signer_name(&setup, false).is_some() {
            let code = run_with_timeout(&setup, &["-u", "-h"], Duration::from_secs(180));
            log::info!("uninstall: VB-CABLE removal exit {code:?}");
        } else {
            log::warn!("uninstall: VB-CABLE setup program changed or missing; not run");
        }
    }
    let _ = std::fs::remove_dir_all(data_dir());
    let _ = std::fs::remove_dir_all(elevated_log_dir());
    0
}

/// Ends every other Claude Talk process (same program file name).
pub fn stop_other_instances() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let me = std::process::id();
    let name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "claude-talk.exe".into());
    for p in ct_engine::win::process::snapshot() {
        if p.pid != me && p.exe == name {
            unsafe {
                if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, p.pid) {
                    let _ = TerminateProcess(h, 0);
                    let _ = CloseHandle(h);
                }
            }
        }
    }
    std::thread::sleep(Duration::from_millis(500));
}

/// Removes "start with Windows" entries that start Claude Talk, for this
/// user and (written by early builds when run elevated) for all users.
fn remove_autostart() {
    crate::autostart::remove_entries(true);
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
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).ok()?;
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
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

/// Verifies a file's signature with WinVerifyTrust (no UI; with
/// `revocation`, the whole chain is checked against revocation lists) and
/// returns the signer's name, or `None` if it is not validly signed.
fn signer_name(file: &Path, revocation: bool) -> Option<String> {
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::Cryptography::{
        CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE,
    };
    use windows::Win32::Security::WinTrust::*;
    if !file.exists() {
        return None;
    }
    let path = HSTRING::from(file.as_os_str());
    let mut fi = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: if revocation {
            WTD_REVOKE_WHOLECHAIN
        } else {
            WTD_REVOKE_NONE
        },
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: if revocation {
            WTD_REVOCATION_CHECK_CHAIN
        } else {
            WTD_CACHE_ONLY_URL_RETRIEVAL
        },
        ..Default::default()
    };
    data.Anonymous.pFile = &mut fi;
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let hwnd = HWND(-1isize as *mut _);
    let r = unsafe { WinVerifyTrust(hwnd, &mut action, &mut data as *mut _ as *mut _) };
    let mut name = None;
    if r == 0 {
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let signer = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
                if !signer.is_null() && !(*signer).pasCertChain.is_null() {
                    let cert = (*(*signer).pasCertChain).pCert;
                    let mut buf = [0u16; 256];
                    let n = CertGetNameStringW(
                        cert,
                        CERT_NAME_SIMPLE_DISPLAY_TYPE,
                        0,
                        None,
                        Some(&mut buf),
                    );
                    if n > 1 {
                        name = Some(String::from_utf16_lossy(&buf[..n as usize - 1]));
                    }
                }
            }
        }
        // a valid signature whose name could not be read still counts
        name = name.or(Some(String::new()));
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(hwnd, &mut action, &mut data as *mut _ as *mut _) };
    if let Some(n) = &name {
        log::info!("installer: {} signed by '{n}'", file.display());
    }
    name
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
        devices::patterns(devices::CABLE_CAPTURE_PATTERNS),
        CAPTURE_NAME,
    );
    rename(
        Flow::Render,
        devices::patterns(devices::CABLE_RENDER_PATTERNS),
        RENDER_NAME,
    );
    rename(
        Flow::Render,
        vec![
            "cable in 16ch (vb-audio virtual cable)".into(),
            "claude talk – intern 16ch".into(),
        ],
        &format!("{RENDER_NAME} 16ch"),
    );
}

struct Defaults {
    render: [Option<String>; 3],
    capture: [Option<String>; 3],
}

fn defaults() -> Defaults {
    Defaults {
        render: devices::default_ids(Flow::Render),
        capture: devices::default_ids(Flow::Capture),
    }
}

/// Installing a driver can make Windows pick the new device as default.
/// Put the user's own defaults back, role by role (a separate
/// communications device stays separate).
fn restore_defaults(before: &Defaults) {
    let now = defaults();
    for (was, is) in [
        (&before.render, &now.render),
        (&before.capture, &now.capture),
    ] {
        for role in 0..3 {
            if let (Some(was), Some(is)) = (&was[role], &is[role]) {
                if was != is {
                    match ct_engine::win::policy::set_default_endpoint_role(was, role) {
                        Ok(()) => {
                            log::info!("installer: restored default device {was} (role {role})")
                        }
                        Err(e) => log::warn!("installer: could not restore default device: {e}"),
                    }
                }
            }
        }
    }
}

/// Runs `--install-cable` elevated from the app ("Installer Discords
/// mikrofon"): one Windows prompt. Returns the exit code, or `None` if the
/// user said no to the prompt.
pub fn install_cable_elevated() -> Result<Option<u32>, String> {
    // Only the installed program is elevated: a copy in a folder the user
    // can write to (the portable zip, Downloads) could have planted DLLs
    // next to it.
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let pf = program_files();
    if !here.starts_with(&pf) {
        log::warn!(
            "install cable: {} is not under {}; use the installer",
            here.display(),
            pf.display()
        );
        return Err("error.cableNeedsInstaller".into());
    }
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let file = HSTRING::from(exe.as_os_str());
    let verb = HSTRING::from("runas");
    let params = HSTRING::from("--install-cable");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            // ERROR_CANCELLED (1223): the user said no to the Windows prompt;
            // anything else is a real failure the user must hear about
            if e.code() == windows::core::HRESULT::from_win32(1223) {
                return Ok(None);
            }
            log::warn!("install cable: could not start the installer: {e}");
            return Err("error.cableInstallFailed".into());
        }
        if info.hProcess.is_invalid() {
            return Err("no process".into());
        }
        let waited = WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 5u32;
        if waited == WAIT_OBJECT_0 {
            let _ = GetExitCodeProcess(info.hProcess, &mut code);
        }
        let _ = CloseHandle(info.hProcess);
        Ok(Some(code))
    }
}
