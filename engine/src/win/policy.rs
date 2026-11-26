//! Per-app default audio devices ("App volume and device preferences").
//!
//! Windows has no public API for this. Like EarTrumpet and SoundVolumeView we
//! use the internal `Windows.Media.Internal.AudioPolicyConfig` activation
//! factory. Its interface id changed in build 21390: newer Windows 10/11
//! builds use the "21H2" variant, older ones the "downlevel" variant; the
//! method layout is the same. Settings are stored by Windows per program, so
//! they survive restarts and only need setting while the program runs.
//!
//! Only the *input* device of Discord is changed. The user's normal default
//! devices are never touched, and [`restore_discord`] undoes the change.

use super::devices::{self, DeviceSelector, Flow};
use super::process;
use std::ffi::c_void;
use windows::core::{HRESULT, HSTRING};
use windows::Win32::Media::Audio::{eCapture, eCommunications, eConsole, eMultimedia, ERole};
use windows::Win32::System::WinRT::RoGetActivationFactory;

macro_rules! policy_interface {
    ($name:ident, $iid:literal) => {
        // Declared on IUnknown with IInspectable's three methods spelled out
        // (same vtable layout as an IInspectable-derived interface).
        #[windows_core::interface($iid)]
        pub unsafe trait $name: windows_core::IUnknown {
            fn get_iids(&self) -> HRESULT;
            fn get_runtime_class_name(&self) -> HRESULT;
            fn get_trust_level(&self) -> HRESULT;
            fn add_ctx_volume_change(&self) -> HRESULT;
            fn remove_ctx_volume_changed(&self) -> HRESULT;
            fn add_ringer_vibrate_state_changed(&self) -> HRESULT;
            fn remove_ringer_vibrate_state_change(&self) -> HRESULT;
            fn set_volume_group_gain_for_id(&self) -> HRESULT;
            fn get_volume_group_gain_for_id(&self) -> HRESULT;
            fn get_active_volume_group_for_endpoint_id(&self) -> HRESULT;
            fn get_volume_groups_for_endpoint(&self) -> HRESULT;
            fn get_current_volume_context(&self) -> HRESULT;
            fn set_volume_group_mute_for_id(&self) -> HRESULT;
            fn get_volume_group_mute_for_id(&self) -> HRESULT;
            fn set_ringer_vibrate_state(&self) -> HRESULT;
            fn get_ringer_vibrate_state(&self) -> HRESULT;
            fn set_preferred_chat_application(&self) -> HRESULT;
            fn reset_preferred_chat_application(&self) -> HRESULT;
            fn get_preferred_chat_application(&self) -> HRESULT;
            fn get_current_chat_applications(&self) -> HRESULT;
            fn add_chat_context_changed(&self) -> HRESULT;
            fn remove_chat_context_changed(&self) -> HRESULT;
            fn set_persisted_default_audio_endpoint(
                &self,
                pid: u32,
                flow: i32,
                role: i32,
                device: *mut c_void,
            ) -> HRESULT;
            fn get_persisted_default_audio_endpoint(
                &self,
                pid: u32,
                flow: i32,
                role: i32,
                device: *mut *mut c_void,
            ) -> HRESULT;
            fn clear_all_persisted_application_default_endpoints(&self) -> HRESULT;
        }
    };
}

policy_interface!(IPolicy21H2, "ab3d4648-e242-459f-b02f-541c70306324");
policy_interface!(IPolicyDownlevel, "2a59116d-6c4f-45e0-a74f-707e3fef9258");

enum Factory {
    New(IPolicy21H2),
    Old(IPolicyDownlevel),
}

const CLASS: &str = "Windows.Media.Internal.AudioPolicyConfig";
/// Device-interface class of capture endpoints (DEVINTERFACE_AUDIO_CAPTURE).
const CAPTURE_INTERFACE: &str = "{2eef81be-33fa-4800-9670-1cd474972c3f}";

fn factory() -> windows::core::Result<Factory> {
    let class = HSTRING::from(CLASS);
    let use_new = super::windows_build() >= 21390;
    unsafe {
        if use_new {
            if let Ok(f) = RoGetActivationFactory::<IPolicy21H2>(&class) {
                return Ok(Factory::New(f));
            }
        }
        match RoGetActivationFactory::<IPolicyDownlevel>(&class) {
            Ok(f) => Ok(Factory::Old(f)),
            Err(e) => RoGetActivationFactory::<IPolicy21H2>(&class)
                .map(Factory::New)
                .map_err(|_| e),
        }
    }
}

impl Factory {
    fn set(&self, pid: u32, role: ERole, device: &HSTRING) -> windows::core::Result<()> {
        // HSTRING is passed by value as its handle (null = clear the setting)
        let raw: *mut c_void = unsafe { std::mem::transmute_copy(device) };
        unsafe {
            match self {
                Factory::New(f) => {
                    f.set_persisted_default_audio_endpoint(pid, eCapture.0, role.0, raw)
                }
                Factory::Old(f) => {
                    f.set_persisted_default_audio_endpoint(pid, eCapture.0, role.0, raw)
                }
            }
        }
        .ok()
    }

    fn get(&self, pid: u32, role: ERole) -> windows::core::Result<String> {
        let mut out: *mut c_void = std::ptr::null_mut();
        unsafe {
            match self {
                Factory::New(f) => {
                    f.get_persisted_default_audio_endpoint(pid, eCapture.0, role.0, &mut out)
                }
                Factory::Old(f) => {
                    f.get_persisted_default_audio_endpoint(pid, eCapture.0, role.0, &mut out)
                }
            }
            .ok()?;
            // take ownership of the returned HSTRING
            let h: HSTRING = std::mem::transmute(out);
            Ok(h.to_string())
        }
    }
}

/// `{0.0.1.00000000}.{guid}` → the interface path the policy store expects.
pub fn interface_path(endpoint_id: &str) -> String {
    format!(r"\\?\SWD#MMDEVAPI#{endpoint_id}#{CAPTURE_INTERFACE}")
}

/// Endpoint id of our virtual microphone (the capture side of the cable).
pub fn cable_capture_id() -> Option<String> {
    devices::resolve(&DeviceSelector::FirstMatching(
        Flow::Capture,
        devices::patterns(devices::CABLE_CAPTURE_PATTERNS),
    ))
    .ok()
    .map(|(_, e)| e.id)
}

/// Every running process of the given executables (all of Discord's
/// processes: the setting is stored per program, any of them will do, but
/// setting all is harmless and covers multi-root cases).
pub fn pids_of(exes: &[String]) -> Vec<u32> {
    let names: Vec<String> = exes.iter().map(|e| e.to_lowercase()).collect();
    process::snapshot()
        .into_iter()
        .filter(|p| names.contains(&p.exe))
        .map(|p| p.pid)
        .collect()
}

const ROLES: [ERole; 3] = [eConsole, eMultimedia, eCommunications];

/// The input the user had set for Discord in Windows ("App volume and device
/// preferences") before Claude Talk pointed it at the cable: given back on
/// restore instead of clearing it. The app keeps it in its settings.
static DISCORD_OWN_INPUT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_discord_own_input(path: Option<String>) {
    *DISCORD_OWN_INPUT.lock().unwrap_or_else(|p| p.into_inner()) = path;
}

pub fn discord_own_input() -> Option<String> {
    DISCORD_OWN_INPUT
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

/// Every value Claude Talk itself has given Discord on restore (the user's
/// microphone, D28). Read back later, none of them is the user's own
/// setting.
static DISCORD_GIVEN_BACK: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn set_discord_given_back(paths: Vec<String>) {
    *DISCORD_GIVEN_BACK.lock().unwrap_or_else(|p| p.into_inner()) = paths;
}

pub fn discord_given_back() -> Vec<String> {
    DISCORD_GIVEN_BACK
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

/// A value that must never be taken as the user's own setting again (one the
/// user replaced with a choice in Claude Talk).
pub fn forget_as_own(path: &str) {
    let mut f = FORGOTTEN.lock().unwrap_or_else(|p| p.into_inner());
    if !f.iter().any(|v| v.eq_ignore_ascii_case(path)) {
        f.push(path.to_string());
    }
}

/// Values the user replaced with a choice in Claude Talk (see `forget_as_own`).
static FORGOTTEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn forgotten() -> Vec<String> {
    FORGOTTEN.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

pub fn set_forgotten(paths: Vec<String>) {
    *FORGOTTEN.lock().unwrap_or_else(|p| p.into_inner()) = paths;
}

pub fn is_forgotten(path: &str) -> bool {
    FORGOTTEN
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .any(|v| v.eq_ignore_ascii_case(path))
}

/// Remembered as written, last in the list (the list's last value is the one
/// Claude Talk wrote most recently).
fn note_given_back(path: &str) {
    let mut g = DISCORD_GIVEN_BACK.lock().unwrap_or_else(|p| p.into_inner());
    g.retain(|v| !v.eq_ignore_ascii_case(path));
    g.push(path.to_string());
}

/// What this Claude Talk last wrote as Discord's input (also "" for "Default"
/// and the user's own value, which are not kept in the given-back list).
static LAST_WRITTEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The value Claude Talk wrote for Discord most recently, if any (from an
/// earlier run: the newest in the given-back list).
pub fn last_given_back() -> Option<String> {
    if let Some(v) = LAST_WRITTEN
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
    {
        return Some(v);
    }
    DISCORD_GIVEN_BACK
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .last()
        .cloned()
}

/// What Discord's input should be while Claude Talk doesn't route it: the
/// user's own per-app setting if there was one; otherwise the microphone
/// Claude Talk uses as "you" (taken over from Discord, D28), unless that is
/// Windows' default for every role anyway (then nothing: no hidden setting
/// that would outlive a later change of the default). None: no setting.
pub fn wanted_discord_input() -> Option<String> {
    discord_own_input().or_else(|| {
        let defaults = devices::default_ids(Flow::Capture);
        devices::preferred_microphone()
            .filter(|id| {
                !defaults
                    .iter()
                    .all(|d| d.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(id)))
            })
            .map(|id| interface_path(&id))
    })
}

/// Discord's stored input right now ("" = none; None: Discord isn't running).
pub fn discord_input_now(exes: &[String]) -> Option<String> {
    let f = factory().ok()?;
    let first = *pids_of(exes).first()?;
    f.get(first, eConsole).ok()
}

/// Whether Discord's stored input is what it should be while Claude Talk
/// doesn't route it (None: Discord isn't running).
pub fn discord_input_as_wanted(exes: &[String]) -> Option<bool> {
    let f = factory().ok()?;
    let first = *pids_of(exes).first()?;
    let now = f.get(first, eConsole).ok()?;
    let wanted = wanted_discord_input().unwrap_or_default();
    Some(now.eq_ignore_ascii_case(&wanted))
}

/// The cable was installed by Claude Talk (only Claude Talk points programs
/// at it). For a VB-CABLE the user had before, only Discord's settings for it
/// are removed.
static CABLE_IS_OURS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether Claude Talk installed the cable itself (see `set_cable_is_ours`).
pub fn cable_is_ours() -> bool {
    CABLE_IS_OURS.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn set_cable_is_ours(ours: bool) {
    CABLE_IS_OURS.store(ours, std::sync::atomic::Ordering::Relaxed);
}

/// Points Discord's microphone at our cable. Returns how many processes were
/// updated (0 if Discord is not running – call again when it starts).
pub fn route_discord_to_cable(exes: &[String]) -> windows::core::Result<usize> {
    let Some(id) = cable_capture_id() else {
        return Err(windows::Win32::Foundation::E_FAIL.into());
    };
    let path = HSTRING::from(interface_path(&id));
    let f = factory()?;
    let pids = pids_of(exes);
    // Remember what the user has set for Discord right now (this is only
    // called while Discord is not pointed at the cable): nothing, or a device.
    if let Some(&first) = pids.first() {
        if let Ok(d) = f.get(first, eConsole) {
            // (empty: nothing set now – keep what was saved: the entry may
            // have been removed while Discord was closed, before it could be
            // put back)
            let given = discord_given_back()
                .iter()
                .any(|g| g.eq_ignore_ascii_case(&d))
                || is_forgotten(&d);
            if !d.is_empty() && !given && !d.to_lowercase().contains(&id.to_lowercase()) {
                set_discord_own_input(Some(d));
            }
        }
    }
    let mut n = 0;
    for pid in pids {
        let mut ok = true;
        for role in ROLES {
            ok &= f.set(pid, role, &path).is_ok();
        }
        if ok {
            n += 1;
        }
    }
    Ok(n)
}

/// True if Windows' stored input device for any of the processes is our cable.
pub fn discord_routed_to_cable(exes: &[String]) -> Option<bool> {
    let id = cable_capture_id()?;
    let f = factory().ok()?;
    let pids = pids_of(exes);
    if pids.is_empty() {
        return None;
    }
    // (a value Windows won't let us read is unknown, not "not the cable")
    let mut known = false;
    for &pid in &pids {
        if let Ok(d) = f.get(pid, eConsole) {
            known = true;
            if d.to_lowercase().contains(&id.to_lowercase()) {
                return Some(true);
            }
        }
    }
    known.then_some(false)
}

/// Gives Discord its normal microphone back. Works while Discord runs (via
/// the policy API) and when it does not (removes the stored entries that
/// point at our cable from the registry where Windows keeps them).
pub fn restore_discord(exes: &[String]) -> usize {
    let mut n = 0;
    if let Ok(f) = factory() {
        // (see `wanted_discord_input`: with Discord on "Default", nothing
        // else would give it back the headset the user had picked there)
        let own = discord_own_input();
        let wanted = wanted_discord_input();
        let back = HSTRING::from(wanted.clone().unwrap_or_default());
        for pid in pids_of(exes) {
            for role in ROLES {
                if f.set(pid, role, &back).is_ok() {
                    n += 1;
                    *LAST_WRITTEN.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(wanted.clone().unwrap_or_default());
                    // remembered once really written (and never as the
                    // user's own setting)
                    if own.is_none() {
                        if let Some(w) = &wanted {
                            note_given_back(w);
                        }
                    }
                }
            }
        }
    }
    n + remove_stored_cable_entries()
}

/// Windows stores per-app devices under this key; each value name contains
/// the program path and the value data the device interface path.
const STORE: &str =
    r"Software\Microsoft\Internet Explorer\LowRegistry\Audio\PolicyConfig\PropertyStore";

/// Newer Windows (24H2 / Server 2025) keeps per-program devices here: one
/// subkey per program, one value per role pointing at a device.
const STORE_NEW: &str = r"Software\Microsoft\Multimedia\Audio\DefaultEndpoint";

/// Removes Discord's stored per-program settings that point at our cable, in
/// both places Windows uses. Other settings – a program's output device, or
/// another program's use of a VB-CABLE the user already had – stay untouched.
fn remove_stored_cable_entries() -> usize {
    let Some(id) = cable_capture_id() else {
        return 0;
    };
    let id = id.to_lowercase();
    [STORE, STORE_NEW]
        .iter()
        .map(|store| unsafe { remove_values_pointing_at(store, &id) })
        .sum()
}

unsafe fn remove_values_pointing_at(store: &str, endpoint_id: &str) -> usize {
    use windows::Win32::System::Registry::*;
    let mut root = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(store),
            None,
            KEY_READ,
            &mut root,
        )
    }
    .is_err()
    {
        return 0;
    }
    let mut subkeys = Vec::new();
    let mut i = 0u32;
    loop {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        let r = unsafe {
            RegEnumKeyExW(
                root,
                i,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if r.is_err() {
            break;
        }
        subkeys.push(String::from_utf16_lossy(&name[..len as usize]));
        i += 1;
    }
    let mut removed = 0;
    for sub in subkeys {
        let mut key = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                root,
                &HSTRING::from(&sub),
                None,
                KEY_READ | KEY_SET_VALUE,
                &mut key,
            )
        }
        .is_err()
        {
            continue;
        }
        let ours = CABLE_IS_OURS.load(std::sync::atomic::Ordering::Relaxed);
        // the program a subkey is for: in its name, or in its default value
        let discord_key = sub.to_lowercase().contains("discord") || {
            let mut data = vec![0u8; 4096];
            let mut dlen = data.len() as u32;
            let r = unsafe {
                RegQueryValueExW(
                    key,
                    windows::core::PCWSTR::null(),
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut dlen),
                )
            };
            r.is_ok() && {
                let words: Vec<u16> = data[..dlen as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&words)
                    .to_lowercase()
                    .contains("discord")
            }
        };
        let mut doomed = Vec::new();
        let mut j = 0u32;
        loop {
            let mut vname = vec![0u16; 16384];
            let mut vlen = vname.len() as u32;
            let mut data = vec![0u8; 65536];
            let mut dlen = data.len() as u32;
            let r = unsafe {
                RegEnumValueW(
                    key,
                    j,
                    Some(windows::core::PWSTR(vname.as_mut_ptr())),
                    &mut vlen,
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut dlen),
                )
            };
            if r == windows::Win32::Foundation::ERROR_MORE_DATA {
                j += 1; // too big to be a device path; look at the next
                continue;
            }
            if r.is_err() {
                break;
            }
            let words: Vec<u16> = data[..dlen as usize]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            // A cable Claude Talk installed: only Claude Talk points programs
            // at it. A VB-CABLE the user had before: only Discord's entries
            // (another program's use of it is the user's).
            let is_discord = ours
                || discord_key
                || String::from_utf16_lossy(&vname[..vlen as usize])
                    .to_lowercase()
                    .contains("discord");
            if is_discord
                && String::from_utf16_lossy(&words)
                    .to_lowercase()
                    .contains(endpoint_id)
            {
                doomed.push(vname[..vlen as usize].to_vec());
            }
            j += 1;
        }
        for mut v in doomed {
            v.push(0);
            if unsafe { RegDeleteValueW(key, windows::core::PCWSTR(v.as_ptr())) }.is_ok() {
                removed += 1;
            }
        }
        let _ = unsafe { RegCloseKey(key) };
    }
    let _ = unsafe { RegCloseKey(root) };
    removed
}

/// `IPolicyConfig` (undocumented, used by the Sound control panel and many
/// tools) – only `SetDefaultEndpoint` is used, to put the user's own default
/// devices back if a driver install changed them.
#[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows_core::IUnknown {
    fn get_mix_format(&self) -> HRESULT;
    fn get_device_format(&self) -> HRESULT;
    fn reset_device_format(&self) -> HRESULT;
    fn set_device_format(
        &self,
        device_id: windows::core::PCWSTR,
        endpoint_format: *const windows::Win32::Media::Audio::WAVEFORMATEX,
        mix_format: *const windows::Win32::Media::Audio::WAVEFORMATEX,
    ) -> HRESULT;
    fn get_processing_period(&self) -> HRESULT;
    fn set_processing_period(&self) -> HRESULT;
    fn get_share_mode(&self) -> HRESULT;
    fn set_share_mode(&self) -> HRESULT;
    fn get_property_value(&self) -> HRESULT;
    fn set_property_value(&self) -> HRESULT;
    fn set_default_endpoint(&self, device_id: windows::core::PCWSTR, role: i32) -> HRESULT;
    fn set_endpoint_visibility(&self) -> HRESULT;
}

const CLSID_POLICY_CONFIG: windows::core::GUID =
    windows::core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Makes `endpoint_id` the default device for one role (index into
/// console, multimedia, communications – as `devices::default_ids`).
pub fn set_default_endpoint_role(endpoint_id: &str, role: usize) -> windows::core::Result<()> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    let pc: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }?;
    let id = HSTRING::from(endpoint_id);
    let role = ROLES.get(role).copied().unwrap_or(eConsole);
    unsafe { pc.set_default_endpoint(windows::core::PCWSTR(id.as_ptr()), role.0) }.ok()
}

/// Makes `endpoint_id` the default device for all roles.
pub fn set_default_endpoint(endpoint_id: &str) -> windows::core::Result<()> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    let pc: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }?;
    let id = HSTRING::from(endpoint_id);
    for role in ROLES {
        unsafe { pc.set_default_endpoint(windows::core::PCWSTR(id.as_ptr()), role.0) }.ok()?;
    }
    Ok(())
}

/// Sets a device's shared-mode format to 16-bit stereo at `rate` (what the
/// Sound control panel's "Default format" does). Used by the robustness test
/// to change a sample rate while Claude Talk runs.
pub fn set_device_rate(endpoint_id: &str, rate: u32) -> windows::core::Result<()> {
    use windows::Win32::Media::Audio::{
        WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
    };
    use windows::Win32::Media::KernelStreaming::{
        KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
    };
    use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    let make = |bits: u16, sub| WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: 2,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * 2 * (bits as u32 / 8),
            nBlockAlign: 2 * bits / 8,
            wBitsPerSample: bits,
            cbSize: 22,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: bits,
        },
        dwChannelMask: SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
        SubFormat: sub,
    };
    let device = make(16, KSDATAFORMAT_SUBTYPE_PCM);
    let mix = make(32, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);
    let pc: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }?;
    let id = HSTRING::from(endpoint_id);
    unsafe {
        pc.set_device_format(
            windows::core::PCWSTR(id.as_ptr()),
            &device as *const _ as *const WAVEFORMATEX,
            &mix as *const _ as *const WAVEFORMATEX,
        )
    }
    .ok()
}

/// The device Windows last made the default for `role` (0 console,
/// 1 multimedia, 2 communications), as an endpoint id, leaving out
/// `exclude`; also a device that is unplugged right now. Windows keeps the
/// time of each default change per device (`Role:N`, a SYSTEMTIME) under
/// HKLM\…\MMDevices\Audio\{Render|Capture}\{guid}, readable by every user.
pub fn last_default_by_windows(render: bool, role: usize, exclude: &[String]) -> Option<String> {
    use windows::Win32::System::Registry::*;
    let flow = if render { "Render" } else { "Capture" };
    let prefix = if render {
        "{0.0.0.00000000}."
    } else {
        "{0.0.1.00000000}."
    };
    let path = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\{flow}");
    let mut root = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(path),
            None,
            KEY_READ,
            &mut root,
        )
    }
    .is_err()
    {
        return None;
    }
    let value = HSTRING::from(format!("Role:{role}"));
    let mut best: Option<([u16; 7], String)> = None;
    let mut i = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        let r = unsafe {
            RegEnumKeyExW(
                root,
                i,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
        };
        if r.is_err() {
            break;
        }
        i += 1;
        let guid = String::from_utf16_lossy(&name[..len as usize]);
        let id = format!("{prefix}{guid}");
        if exclude.iter().any(|e| e.eq_ignore_ascii_case(&id)) {
            continue;
        }
        let mut data = [0u8; 16];
        let mut dlen = data.len() as u32;
        let r = unsafe {
            RegGetValueW(
                root,
                &HSTRING::from(&guid),
                &value,
                RRF_RT_REG_BINARY,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&mut dlen),
            )
        };
        if r.is_err() || dlen != 16 {
            continue;
        }
        let w: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        // year, month, day, hour, minute, second, ms (w[2] is the weekday)
        let when = [w[0], w[1], w[3], w[4], w[5], w[6], w[7]];
        if best.as_ref().is_none_or(|(b, _)| when > *b) {
            best = Some((when, id));
        }
    }
    unsafe {
        let _ = RegCloseKey(root);
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interface_path_format() {
        assert_eq!(
            interface_path("{0.0.1.00000000}.{abc}"),
            r"\\?\SWD#MMDEVAPI#{0.0.1.00000000}.{abc}#{2eef81be-33fa-4800-9670-1cd474972c3f}"
        );
    }
}
