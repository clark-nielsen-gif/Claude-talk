//! Who is recording from which microphone right now (`IAudioSessionManager2`).
//! Used to *verify* that Discord really records from our cable, instead of
//! assuming it.

use super::devices::{self, Flow};
use super::process;
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eCapture, eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSession {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub pid: u32,
    pub exe: String,
    pub active: bool,
}

/// All recording sessions on all active capture endpoints.
pub fn capture_sessions() -> windows::core::Result<Vec<CaptureSession>> {
    sessions(Flow::Capture)
}

/// All audio sessions on all active endpoints of one direction (playback
/// sessions: which programs are making sound).
pub fn sessions(flow: Flow) -> windows::core::Result<Vec<CaptureSession>> {
    let procs = process::snapshot();
    let exe_of = |pid: u32| {
        procs
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.exe.clone())
            .unwrap_or_default()
    };
    let names: std::collections::HashMap<String, String> = devices::list(flow)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.id, e.name))
        .collect();
    let mut out = Vec::new();
    unsafe {
        let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let data_flow = match flow {
            Flow::Capture => eCapture,
            Flow::Render => eRender,
        };
        let coll = e.EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE)?;
        for i in 0..coll.GetCount()? {
            let dev = coll.Item(i)?;
            let id = super::devices::device_id(&dev);
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(sessions) = mgr.GetSessionEnumerator() else {
                continue;
            };
            for k in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(ctl) = sessions.GetSession(k) else {
                    continue;
                };
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                let pid = ctl2.GetProcessId().unwrap_or(0);
                if pid == 0 {
                    continue;
                }
                let active = ctl
                    .GetState()
                    .map(|s| s == AudioSessionStateActive)
                    .unwrap_or(false);
                out.push(CaptureSession {
                    endpoint_name: names.get(&id).cloned().unwrap_or_default(),
                    endpoint_id: id.clone(),
                    pid,
                    exe: exe_of(pid),
                    active,
                });
            }
        }
    }
    Ok(out)
}

/// Where Discord is recording from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscordMic {
    /// Discord records from our cable (it hears Claude).
    Cable,
    /// Discord records from a different microphone.
    Other,
    /// Discord is not recording right now (not in a call), or not running.
    Unknown,
}

pub fn discord_mic(exes: &[String], cable_id: Option<&str>) -> DiscordMic {
    let names: Vec<String> = exes.iter().map(|s| s.to_lowercase()).collect();
    let Ok(all) = capture_sessions() else {
        return DiscordMic::Unknown;
    };
    let discord: Vec<&CaptureSession> = all
        .iter()
        .filter(|s| s.active && names.contains(&s.exe))
        .collect();
    if discord.is_empty() {
        return DiscordMic::Unknown;
    }
    match cable_id {
        Some(id)
            if discord
                .iter()
                .any(|s| s.endpoint_id.eq_ignore_ascii_case(id)) =>
        {
            DiscordMic::Cable
        }
        _ => DiscordMic::Other,
    }
}
