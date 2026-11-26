//! "Start with Windows" (on by default, can be turned off).
//!
//! Written by Claude Talk itself, always for the current user only
//! (HKCU\...\Run). The autostart plugin's default writes to HKLM – for
//! every user of the computer – whenever the program happens to run
//! elevated, and leaves the path unquoted.
use tauri::AppHandle;

pub const VALUE_NAME: &str = "Claude Talk";

#[cfg(windows)]
pub fn apply(_app: &AppHandle, enabled: bool) {
    if !enabled {
        remove_entries(false);
        log::info!("autostart off");
        return;
    }
    // (over)writes our own value only: a "disabled" choice in Task
    // Manager (StartupApproved) stays the user's
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let command = format!("\"{}\" --minimized", exe.display());
    let wide: Vec<u16> = command.encode_utf16().chain([0]).collect();
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            &HSTRING::from(VALUE_NAME),
            REG_SZ.0,
            Some(wide.as_ptr() as *const _),
            (wide.len() * 2) as u32,
        )
    };
    if r.is_ok() {
        log::info!("autostart on");
    } else {
        log::warn!("autostart could not be set: {r:?}");
    }
}

#[cfg(not(windows))]
pub fn apply(_app: &AppHandle, _enabled: bool) {}

/// Whether Windows will really start Claude Talk at the next sign-in: our
/// Run value points at this program, and it is not turned off under the
/// startup apps (StartupApproved: an odd first byte means disabled).
#[cfg(windows)]
pub fn will_start() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_ANY};
    let read = |path: &str| -> Option<Vec<u8>> {
        let (key, name) = (HSTRING::from(path), HSTRING::from(VALUE_NAME));
        let mut len = 0u32;
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_ANY,
                None,
                None,
                Some(&mut len),
            )
        }
        .ok()
        .ok()?;
        let mut buf = vec![0u8; len as usize];
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_ANY,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut len),
            )
        }
        .ok()
        .ok()?;
        buf.truncate(len as usize);
        Some(buf)
    };
    let Some(run) = read(r"Software\Microsoft\Windows\CurrentVersion\Run") else {
        return false;
    };
    let wide: Vec<u16> = run
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let command = String::from_utf16_lossy(&wide).to_lowercase();
    let ours = std::env::current_exe()
        .map(|e| command.contains(&e.to_string_lossy().to_lowercase()))
        .unwrap_or(false);
    let disabled = read(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run")
        .and_then(|v| v.first().copied())
        .is_some_and(|b| b % 2 == 1);
    ours && !disabled
}

#[cfg(not(windows))]
pub fn will_start() -> bool {
    false
}

/// Removes every Run / StartupApproved entry that starts Claude Talk,
/// whatever name it was written under (also the plugin's old ones).
#[cfg(windows)]
pub fn remove_entries(all_users: bool) {
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::System::Registry::*;
    let roots: &[HKEY] = if all_users {
        &[HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
    } else {
        &[HKEY_CURRENT_USER]
    };
    for (root, path) in roots.iter().flat_map(|r| {
        [
            (*r, r"Software\Microsoft\Windows\CurrentVersion\Run"),
            (
                *r,
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            ),
        ]
    }) {
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                root,
                &HSTRING::from(path),
                None,
                KEY_READ | KEY_SET_VALUE,
                &mut key,
            )
            .is_err()
            {
                continue;
            }
            let mut doomed: Vec<Vec<u16>> = Vec::new();
            let mut i = 0u32;
            loop {
                let mut name = vec![0u16; 16384];
                let mut nlen = name.len() as u32;
                let mut data = vec![0u8; 65536];
                let mut dlen = data.len() as u32;
                let r = RegEnumValueW(
                    key,
                    i,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut nlen,
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut dlen),
                );
                if r == windows::Win32::Foundation::ERROR_MORE_DATA {
                    i += 1; // a huge value is not ours; look at the next
                    continue;
                }
                if r.is_err() {
                    break;
                }
                let n = String::from_utf16_lossy(&name[..nlen as usize]).to_lowercase();
                let words: Vec<u16> = data[..dlen as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let d = String::from_utf16_lossy(&words).to_lowercase();
                if d.contains("claude-talk.exe") || n == "claude talk" || n == "claude-talk" {
                    doomed.push(name[..nlen as usize].iter().copied().chain([0]).collect());
                }
                i += 1;
            }
            for n in doomed {
                let _ = RegDeleteValueW(key, PCWSTR(n.as_ptr()));
            }
            let _ = RegCloseKey(key);
        }
    }
}
