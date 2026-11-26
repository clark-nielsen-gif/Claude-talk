//! Audio endpoint discovery: names, defaults, form factor (headphones or
//! speakers), and renaming of our virtual microphone.

use windows::core::{Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32, PROPVARIANT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ, STGM_READWRITE,
};
use windows::Win32::System::Variant::VT_LPWSTR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Render,
    Capture,
}

/// Physical kind of an endpoint (subset of `EndpointFormFactor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormFactor {
    Speakers,
    Headphones,
    Headset,
    Microphone,
    Other,
}

impl FormFactor {
    /// True if sound from this output can reach a microphone through the air.
    pub fn is_open_air(self) -> bool {
        matches!(self, FormFactor::Speakers | FormFactor::Other)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub flow: Flow,
    pub form_factor: FormFactor,
}

/// How a stream finds its device. Resolved again after every failure, so a
/// replugged headset or a changed default device is picked up.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceSelector {
    /// The Windows default device for this flow (console role).
    Default(Flow),
    /// The Windows default communications device for this flow.
    DefaultCommunications(Flow),
    /// A specific endpoint id.
    Id(String),
    /// The first active endpoint whose name contains this text.
    NameContains(Flow, String),
    /// The Windows default device, unless its name contains one of the
    /// patterns; then the first other active device. Used for the
    /// microphone so the engine can never record its own virtual cable
    /// (which would feed "Til Discord" back into itself).
    DefaultExcluding(Flow, Vec<String>),
    /// The first active endpoint matching the first pattern that matches
    /// anything (patterns in order of preference).
    FirstMatching(Flow, Vec<String>),
}

/// Name fragments of the virtual devices Claude Talk itself feeds (our names
/// after renaming, or VB-CABLE's own names). Only VB-CABLE itself matches:
/// VB-CABLE A/B, Hi-Fi Cable and VoiceMeeter keep their own roles.
pub const OWN_VIRTUAL_DEVICES: &[&str] = &["claude talk", "(vb-audio virtual cable)"];

/// Inputs that carry what the computer plays rather than a voice. They are
/// never picked as the microphone and cannot be added as a mixer source
/// (they would let people hear themselves).
pub const NOT_A_MICROPHONE: &[&str] = &[
    "claude talk",
    "(vb-audio virtual cable)",
    "stereo mix",
    "stereomix",
    "what u hear",
    "wave out",
    "loopback",
    "rec. playback",
    "mix (",
];

/// Our virtual microphone (Discord records it), most preferred first.
pub const CABLE_CAPTURE_PATTERNS: &[&str] = &[
    "claude talk – til discord",
    "cable output (vb-audio virtual cable)",
];
/// The playback side of the cable (we play into it). The trailing " (" keeps
/// the 16-channel twin ("… 16ch") out.
pub const CABLE_RENDER_PATTERNS: &[&str] = &[
    "claude talk – intern (",
    "cable input (vb-audio virtual cable)",
];

pub fn patterns(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

fn excluded(name: &str, patterns: &[String]) -> bool {
    let n = name.to_lowercase();
    patterns.iter().any(|p| n.contains(&p.to_lowercase()))
}

/// HRESULT_FROM_WIN32(ERROR_NOT_FOUND)
const NOT_FOUND: windows::core::HRESULT = windows::core::HRESULT(0x80070490u32 as i32);

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn data_flow(f: Flow) -> EDataFlow {
    match f {
        Flow::Render => eRender,
        Flow::Capture => eCapture,
    }
}

fn pwstr_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

pub(crate) fn device_id(dev: &IMMDevice) -> String {
    unsafe { dev.GetId() }
        .map(pwstr_to_string)
        .unwrap_or_default()
}

fn read_string(dev: &IMMDevice, key: &PROPERTYKEY) -> String {
    unsafe {
        let Ok(store) = dev.OpenPropertyStore(STGM_READ) else {
            return String::new();
        };
        let Ok(mut v) = store.GetValue(key) else {
            return String::new();
        };
        let s = PropVariantToStringAlloc(&v)
            .map(pwstr_to_string)
            .unwrap_or_default();
        let _ = PropVariantClear(&mut v);
        s
    }
}

fn read_u32(dev: &IMMDevice, key: &PROPERTYKEY) -> Option<u32> {
    unsafe {
        let store = dev.OpenPropertyStore(STGM_READ).ok()?;
        let mut v = store.GetValue(key).ok()?;
        let r = PropVariantToUInt32(&v).ok();
        let _ = PropVariantClear(&mut v);
        r
    }
}

pub(crate) fn describe(dev: &IMMDevice, flow: Flow) -> Endpoint {
    let ff = match read_u32(dev, &PKEY_AudioEndpoint_FormFactor) {
        Some(1) => FormFactor::Speakers,
        Some(3) => FormFactor::Headphones,
        Some(5) => FormFactor::Headset,
        Some(4) => FormFactor::Microphone,
        _ => FormFactor::Other,
    };
    Endpoint {
        id: device_id(dev),
        name: read_string(dev, &PKEY_Device_FriendlyName),
        flow,
        form_factor: ff,
    }
}

/// All active endpoints of a flow.
pub fn list(flow: Flow) -> windows::core::Result<Vec<Endpoint>> {
    let e = enumerator()?;
    let coll = unsafe { e.EnumAudioEndpoints(data_flow(flow), DEVICE_STATE_ACTIVE) }?;
    let n = unsafe { coll.GetCount() }?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        if let Ok(dev) = unsafe { coll.Item(i) } {
            out.push(describe(&dev, flow));
        }
    }
    Ok(out)
}

/// Resolves a selector to a device that is active right now.
pub fn resolve(sel: &DeviceSelector) -> windows::core::Result<(IMMDevice, Endpoint)> {
    let e = enumerator()?;
    let (dev, flow) = match sel {
        DeviceSelector::Default(f) => (
            unsafe { e.GetDefaultAudioEndpoint(data_flow(*f), eConsole) }?,
            *f,
        ),
        DeviceSelector::DefaultCommunications(f) => (
            unsafe { e.GetDefaultAudioEndpoint(data_flow(*f), eCommunications) }?,
            *f,
        ),
        DeviceSelector::Id(id) => {
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            let dev = unsafe { e.GetDevice(PCWSTR(wide.as_ptr())) }?;
            let flow = match unsafe {
                dev.cast::<windows::Win32::Media::Audio::IMMEndpoint>()?
                    .GetDataFlow()
            }? {
                f if f == eCapture => Flow::Capture,
                _ => Flow::Render,
            };
            (dev, flow)
        }
        DeviceSelector::DefaultExcluding(f, patterns) => {
            let def = unsafe { e.GetDefaultAudioEndpoint(data_flow(*f), eConsole) }.ok();
            let def_ok = def
                .as_ref()
                .filter(|d| !excluded(&read_string(d, &PKEY_Device_FriendlyName), patterns))
                .cloned();
            match def_ok {
                Some(d) => (d, *f),
                None => {
                    let coll = unsafe { e.EnumAudioEndpoints(data_flow(*f), DEVICE_STATE_ACTIVE) }?;
                    let n = unsafe { coll.GetCount() }?;
                    let mut found = None;
                    for i in 0..n {
                        let dev = unsafe { coll.Item(i) }?;
                        if !excluded(&read_string(&dev, &PKEY_Device_FriendlyName), patterns) {
                            found = Some(dev);
                            break;
                        }
                    }
                    (
                        found.ok_or_else(|| windows::core::Error::from(NOT_FOUND))?,
                        *f,
                    )
                }
            }
        }
        DeviceSelector::FirstMatching(f, patterns) => {
            let coll = unsafe { e.EnumAudioEndpoints(data_flow(*f), DEVICE_STATE_ACTIVE) }?;
            let n = unsafe { coll.GetCount() }?;
            let mut all = Vec::new();
            for i in 0..n {
                let dev = unsafe { coll.Item(i) }?;
                let name = read_string(&dev, &PKEY_Device_FriendlyName).to_lowercase();
                all.push((dev, name));
            }
            let found = patterns.iter().find_map(|p| {
                all.iter()
                    .find(|(_, name)| name.contains(&p.to_lowercase()))
                    .map(|(d, _)| d.clone())
            });
            (
                found.ok_or_else(|| windows::core::Error::from(NOT_FOUND))?,
                *f,
            )
        }
        DeviceSelector::NameContains(f, text) => {
            let coll = unsafe { e.EnumAudioEndpoints(data_flow(*f), DEVICE_STATE_ACTIVE) }?;
            let n = unsafe { coll.GetCount() }?;
            let needle = text.to_lowercase();
            let mut found = None;
            for i in 0..n {
                let dev = unsafe { coll.Item(i) }?;
                if read_string(&dev, &PKEY_Device_FriendlyName)
                    .to_lowercase()
                    .contains(&needle)
                {
                    found = Some(dev);
                    break;
                }
            }
            (
                found.ok_or_else(|| windows::core::Error::from(NOT_FOUND))?,
                *f,
            )
        }
    };
    if unsafe { dev.GetState() }? != DEVICE_STATE_ACTIVE {
        return Err(NOT_FOUND.into());
    }
    let info = describe(&dev, flow);
    Ok((dev, info))
}

/// Id of the current default device of a flow (console role).
pub fn default_id(flow: Flow) -> Option<String> {
    let e = enumerator().ok()?;
    let dev = unsafe { e.GetDefaultAudioEndpoint(data_flow(flow), eConsole) }.ok()?;
    Some(device_id(&dev))
}

/// Property keys for renaming (see `DECISIONS.md` D1): Windows shows
/// "<description> (<interface name>)".
const PKEY_DEVICE_DESC: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 2,
};
const PKEY_DEVICE_INTERFACE_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 6,
};

/// Renames an endpoint the same way the Sound control panel does. Needs an
/// elevated caller (the installer).
pub fn rename(dev: &IMMDevice, description: &str, interface: &str) -> windows::core::Result<()> {
    unsafe {
        let store = dev.OpenPropertyStore(STGM_READWRITE)?;
        for (key, text) in [
            (&PKEY_DEVICE_DESC, description),
            (&PKEY_DEVICE_INTERFACE_NAME, interface),
        ] {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            // ManuallyDrop: the string belongs to `wide`, not to COM; letting
            // PROPVARIANT drop would CoTaskMemFree it.
            let mut v = std::mem::ManuallyDrop::new(PROPVARIANT::default());
            (*v.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*v.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(wide.as_ptr() as *mut u16);
            store.SetValue(key, &*v)?;
        }
        store.Commit()
    }
}
