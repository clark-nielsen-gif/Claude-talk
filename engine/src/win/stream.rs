//! Supervised WASAPI streams.
//!
//! A stream thread repeats: resolve target → open → run event loop → close,
//! with back-off between attempts. All allocation and COM set-up happens in
//! "open"; the event loop itself only moves samples between WASAPI and the
//! lock-free [`SourceFanout`]/[`OutputMixer`] and updates atomics. Nothing is
//! logged from the stream thread; the control thread reads [`StreamStatus`].

use super::devices::{self, DeviceSelector};
use super::process::{self, ProcHandle};
use super::ComGuard;
use crate::mixer::{OutputMixer, SourceFanout};
use crate::{ENGINE_CHANNELS, ENGINE_RATE};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::core::{implement, Interface, IUnknown, Ref, HRESULT};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{IAgileObject, IAgileObject_Impl, BLOB, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};
use windows::Win32::System::Variant::VT_BLOB;

/// What a capture stream records.
#[derive(Clone, Debug)]
pub enum CaptureTarget {
    /// A capture device (microphone, virtual cable output).
    Device(DeviceSelector),
    /// Everything a render device plays ("system loopback").
    DeviceLoopback(DeviceSelector),
    /// Only the sound of one program and its child processes.
    Process(ProcessTarget),
}

#[derive(Clone, Debug)]
pub enum ProcessTarget {
    /// Root process found by executable name (e.g. `Discord.exe`); found
    /// again automatically when the program restarts.
    ExeNames(Vec<String>),
    /// A PID supplied by the app (the Claude window's WebView2 browser
    /// process). 0 means "not running yet".
    Pid(Arc<AtomicU32>),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    Starting = 0,
    Running = 1,
    WaitingForDevice = 2,
    WaitingForProcess = 3,
    Failed = 4,
    Stopped = 5,
}

impl StreamState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => StreamState::Running,
            2 => StreamState::WaitingForDevice,
            3 => StreamState::WaitingForProcess,
            4 => StreamState::Failed,
            5 => StreamState::Stopped,
            _ => StreamState::Starting,
        }
    }
}

/// Live state of a stream, shared with the control thread.
#[derive(Debug, Default)]
pub struct StreamStatus {
    state: AtomicU8,
    pub last_error: AtomicI32,
    pub pid: AtomicU32,
    pub period_frames: AtomicU32,
    pub restarts: AtomicU32,
    pub discontinuities: AtomicU64,
    pub silence_filled_frames: AtomicU64,
    /// Set by the control thread to make the stream reopen its device (e.g.
    /// the Windows default device changed).
    pub restart_requested: AtomicBool,
    pub device_name: Mutex<String>,
}

impl StreamStatus {
    pub fn state(&self) -> StreamState {
        StreamState::from_u8(self.state.load(Ordering::Relaxed))
    }
    fn set(&self, s: StreamState) {
        self.state.store(s as u8, Ordering::Relaxed);
    }
}

/// Owns a stream thread; stops and joins it when dropped.
pub struct StreamHandle {
    stop: Arc<AtomicBool>,
    wake: Arc<Event>,
    thread: Option<JoinHandle<()>>,
    pub status: Arc<StreamStatus>,
}

impl StreamHandle {
    pub fn request_restart(&self) {
        self.status.restart_requested.store(true, Ordering::Relaxed);
        self.wake.set();
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake.set();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Auto-reset Win32 event.
pub struct Event(HANDLE);
unsafe impl Send for Event {}
unsafe impl Sync for Event {}
impl Event {
    fn new() -> windows::core::Result<Event> {
        Ok(Event(unsafe { CreateEventW(None, false, false, None) }?))
    }
    fn set(&self) {
        unsafe {
            let _ = SetEvent(self.0);
        }
    }
}
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct Mmcss(HANDLE);
impl Mmcss {
    fn enter() -> Option<Mmcss> {
        let mut idx = 0u32;
        unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut idx) }.ok().map(Mmcss)
    }
}
impl Drop for Mmcss {
    fn drop(&mut self) {
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

fn engine_format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: ENGINE_CHANNELS as u16,
        nSamplesPerSec: ENGINE_RATE,
        nAvgBytesPerSec: ENGINE_RATE * ENGINE_CHANNELS as u32 * 4,
        nBlockAlign: (ENGINE_CHANNELS * 4) as u16,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

/// Why one run of a stream ended.
enum Ended {
    Stop,
    Restart,
    WaitDevice(HRESULT),
    WaitProcess,
    Failed(HRESULT),
}

impl From<windows::core::Error> for Ended {
    fn from(e: windows::core::Error) -> Self {
        // Device gone / changed: wait for it to come back rather than fail.
        let code = e.code();
        const DEVICE_INVALIDATED: HRESULT = HRESULT(0x88890004u32 as i32);
        const RESOURCES_INVALIDATED: HRESULT = HRESULT(0x88890026u32 as i32);
        const SERVICE_NOT_RUNNING: HRESULT = HRESULT(0x88890010u32 as i32);
        const NOT_FOUND: HRESULT = HRESULT(0x80070490u32 as i32);
        if code == DEVICE_INVALIDATED || code == RESOURCES_INVALIDATED || code == SERVICE_NOT_RUNNING || code == NOT_FOUND {
            Ended::WaitDevice(code)
        } else {
            Ended::Failed(code)
        }
    }
}

struct Ctx {
    stop: Arc<AtomicBool>,
    wake: Arc<Event>,
    status: Arc<StreamStatus>,
}

impl Ctx {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    fn take_restart(&self) -> bool {
        self.status.restart_requested.swap(false, Ordering::Relaxed)
    }
    /// Sleeps up to `d`, waking early on stop/restart. Returns false on stop.
    fn sleep(&self, d: Duration) -> bool {
        unsafe { WaitForSingleObject(self.wake.0, d.as_millis() as u32) };
        !self.stopping()
    }
}

fn supervise(name: &'static str, ctx: Ctx, mut run: impl FnMut(&Ctx) -> Ended + Send + 'static) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("audio-{name}"))
        .spawn(move || {
            let _com = ComGuard::new();
            let _mmcss = Mmcss::enter();
            let mut backoff = Duration::from_millis(100);
            loop {
                if ctx.stopping() {
                    break;
                }
                ctx.status.set(StreamState::Starting);
                let started = Instant::now();
                let ended = catch_unwind(AssertUnwindSafe(|| run(&ctx))).unwrap_or(Ended::Failed(HRESULT(-1)));
                match ended {
                    Ended::Stop => break,
                    Ended::Restart => {
                        backoff = Duration::from_millis(50);
                    }
                    Ended::WaitDevice(hr) => {
                        ctx.status.last_error.store(hr.0, Ordering::Relaxed);
                        ctx.status.set(StreamState::WaitingForDevice);
                    }
                    Ended::WaitProcess => ctx.status.set(StreamState::WaitingForProcess),
                    Ended::Failed(hr) => {
                        ctx.status.last_error.store(hr.0, Ordering::Relaxed);
                        ctx.status.set(StreamState::Failed);
                    }
                }
                ctx.status.restarts.fetch_add(1, Ordering::Relaxed);
                // A run that lasted a while resets the back-off.
                if started.elapsed() > Duration::from_secs(5) {
                    backoff = Duration::from_millis(100);
                }
                if !ctx.sleep(backoff) {
                    break;
                }
                backoff = (backoff * 2).min(Duration::from_secs(2));
            }
            ctx.status.set(StreamState::Stopped);
        })
        .expect("spawn audio thread")
}

fn new_ctx() -> (Ctx, Arc<AtomicBool>, Arc<Event>, Arc<StreamStatus>) {
    let stop = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Event::new().expect("CreateEvent"));
    let status = Arc::new(StreamStatus::default());
    (Ctx { stop: stop.clone(), wake: wake.clone(), status: status.clone() }, stop, wake, status)
}

/// Starts a capture stream feeding `fanout`.
pub fn spawn_capture(name: &'static str, target: CaptureTarget, mut fanout: SourceFanout) -> StreamHandle {
    let (ctx, stop, wake, status) = new_ctx();
    let thread = supervise(name, ctx, move |ctx| run_capture(ctx, &target, &mut fanout));
    StreamHandle { stop, wake, thread: Some(thread), status }
}

/// Starts a render stream pulling from `mixer`.
pub fn spawn_render(name: &'static str, device: DeviceSelector, mut mixer: OutputMixer) -> StreamHandle {
    let (ctx, stop, wake, status) = new_ctx();
    let thread = supervise(name, ctx, move |ctx| run_render(ctx, &device, &mut mixer));
    StreamHandle { stop, wake, thread: Some(thread), status }
}

#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct ActivationDone(HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationDone_Impl {
    fn ActivateCompleted(&self, _op: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        unsafe { SetEvent(self.0) }
    }
}

impl IAgileObject_Impl for ActivationDone_Impl {}

/// Opens an `IAudioClient` that records only `pid` and its children.
fn activate_process_loopback(pid: u32) -> windows::core::Result<IAudioClient> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let mut pv = PROPVARIANT::default();
    unsafe {
        (*pv.Anonymous.Anonymous).vt = VT_BLOB;
        (*pv.Anonymous.Anonymous).Anonymous.blob =
            BLOB { cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, pBlobData: &params as *const _ as *mut u8 };
    }
    let done = Event::new()?;
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationDone(done.0).into();
    let op = unsafe { ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&pv), &handler) }?;
    if unsafe { WaitForSingleObject(done.0, 5000) } != WAIT_OBJECT_0 {
        return Err(windows::Win32::Foundation::E_ABORT.into());
    }
    let mut hr = HRESULT(0);
    let mut unk: Option<IUnknown> = None;
    unsafe { op.GetActivateResult(&mut hr, &mut unk) }?;
    hr.ok()?;
    unk.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_NOINTERFACE))?.cast()
}

fn resolve_pid(target: &ProcessTarget) -> Option<u32> {
    match target {
        ProcessTarget::Pid(p) => Some(p.load(Ordering::Relaxed)).filter(|&p| p != 0),
        ProcessTarget::ExeNames(names) => process::find_roots(&process::snapshot(), names).into_iter().next(),
    }
}

fn run_capture(ctx: &Ctx, target: &CaptureTarget, fanout: &mut SourceFanout) -> Ended {
    // ---- open (may allocate) ----
    let mut proc_handle: Option<ProcHandle> = None;
    let fill_silence;
    let client: IAudioClient = match target {
        CaptureTarget::Device(sel) | CaptureTarget::DeviceLoopback(sel) => {
            let (dev, info) = match devices::resolve(sel) {
                Ok(v) => v,
                Err(e) => return Ended::WaitDevice(e.code()),
            };
            *ctx.status.device_name.lock().unwrap_or_else(|p| p.into_inner()) = info.name;
            fill_silence = matches!(target, CaptureTarget::DeviceLoopback(_));
            match unsafe { dev.Activate::<IAudioClient>(CLSCTX_ALL, None) } {
                Ok(c) => c,
                Err(e) => return e.into(),
            }
        }
        CaptureTarget::Process(pt) => {
            let Some(pid) = resolve_pid(pt) else { return Ended::WaitProcess };
            proc_handle = ProcHandle::open(pid);
            if proc_handle.is_none() {
                return Ended::WaitProcess;
            }
            ctx.status.pid.store(pid, Ordering::Relaxed);
            fill_silence = true;
            match activate_process_loopback(pid) {
                Ok(c) => c,
                Err(e) => return Ended::Failed(e.code()),
            }
        }
    };
    let loopback = !matches!(target, CaptureTarget::Device(_));
    let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if loopback {
        flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
    }
    let fmt = engine_format();
    // 20 ms buffer: enough slack for the engine to schedule us.
    if let Err(e) = unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &fmt, None) } {
        return e.into();
    }
    let event = match Event::new() {
        Ok(e) => e,
        Err(e) => return e.into(),
    };
    let capture: IAudioCaptureClient = match unsafe { client.SetEventHandle(event.0).and_then(|_| client.GetService()) } {
        Ok(c) => c,
        Err(e) => return e.into(),
    };
    if let Err(e) = unsafe { client.Start() } {
        return e.into();
    }
    ctx.status.set(StreamState::Running);

    // ---- run (no allocation) ----
    let mut handles = [event.0, ctx.wake.0, HANDLE::default()];
    let n_handles = if let Some(p) = &proc_handle {
        handles[2] = p.0;
        3
    } else {
        2
    };
    let mut last_data = Instant::now();
    let ended = loop {
        let w = unsafe { WaitForMultipleObjects(&handles[..n_handles], false, 30) };
        if ctx.stopping() {
            break Ended::Stop;
        }
        if ctx.take_restart() {
            break Ended::Restart;
        }
        if n_handles == 3 && w.0 == WAIT_OBJECT_0.0 + 2 {
            break Ended::WaitProcess; // the target program exited
        }
        let mut got = false;
        loop {
            let next = match unsafe { capture.GetNextPacketSize() } {
                Ok(n) => n,
                Err(e) => return finish(&client, e.into()),
            };
            if next == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut bflags = 0u32;
            if let Err(e) = unsafe { capture.GetBuffer(&mut data, &mut frames, &mut bflags, None, None) } {
                return finish(&client, e.into());
            }
            if bflags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                ctx.status.discontinuities.fetch_add(1, Ordering::Relaxed);
            }
            if bflags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                fanout.push(None, ENGINE_CHANNELS, frames as usize);
            } else {
                let samples = unsafe { std::slice::from_raw_parts(data as *const f32, frames as usize * ENGINE_CHANNELS) };
                fanout.push(Some(samples), ENGINE_CHANNELS, frames as usize);
            }
            if let Err(e) = unsafe { capture.ReleaseBuffer(frames) } {
                return finish(&client, e.into());
            }
            got = true;
        }
        let now = Instant::now();
        if got {
            last_data = now;
        } else if fill_silence && w == WAIT_TIMEOUT {
            // Loopback streams deliver nothing while the program is silent.
            // Keep the downstream buffers moving with silence so mixing
            // continues and drift control stays locked.
            let gap = now.duration_since(last_data);
            if gap >= Duration::from_millis(30) {
                let frames = (gap.as_secs_f64() * ENGINE_RATE as f64) as usize;
                let frames = frames.min(ENGINE_RATE as usize / 10);
                fanout.push(None, ENGINE_CHANNELS, frames);
                ctx.status.silence_filled_frames.fetch_add(frames as u64, Ordering::Relaxed);
                last_data = now;
            }
        }
    };
    finish(&client, ended)
}

fn finish(client: &IAudioClient, ended: Ended) -> Ended {
    unsafe {
        let _ = client.Stop();
    }
    ended
}

fn run_render(ctx: &Ctx, sel: &DeviceSelector, mixer: &mut OutputMixer) -> Ended {
    // ---- open ----
    let (dev, info) = match devices::resolve(sel) {
        Ok(v) => v,
        Err(e) => return Ended::WaitDevice(e.code()),
    };
    *ctx.status.device_name.lock().unwrap_or_else(|p| p.into_inner()) = info.name;
    let client: IAudioClient = match unsafe { dev.Activate(CLSCTX_ALL, None) } {
        Ok(c) => c,
        Err(e) => return e.into(),
    };
    let fmt = engine_format();
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if let Err(e) = unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, &fmt, None) } {
        return e.into();
    }
    let event = match Event::new() {
        Ok(e) => e,
        Err(e) => return e.into(),
    };
    let (render, buffer_frames): (IAudioRenderClient, u32) =
        match unsafe { client.SetEventHandle(event.0).and_then(|_| Ok((client.GetService()?, client.GetBufferSize()?))) } {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
    ctx.status.period_frames.store(buffer_frames, Ordering::Relaxed);
    // pre-roll one buffer of silence so the first period never underruns
    unsafe {
        if render.GetBuffer(buffer_frames).is_ok() {
            let _ = render.ReleaseBuffer(buffer_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
        }
    }
    if let Err(e) = unsafe { client.Start() } {
        return e.into();
    }
    ctx.status.set(StreamState::Running);

    // ---- run (no allocation) ----
    let handles = [event.0, ctx.wake.0];
    let ended = loop {
        let w = unsafe { WaitForMultipleObjects(&handles, false, 200) };
        if ctx.stopping() {
            break Ended::Stop;
        }
        if ctx.take_restart() {
            break Ended::Restart;
        }
        if w == WAIT_TIMEOUT {
            // The engine stopped calling us (device disappeared or system
            // sleep). Probe the client; an error ends the run.
            if let Err(e) = unsafe { client.GetCurrentPadding() } {
                break e.into();
            }
            continue;
        }
        let padding = match unsafe { client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(e) => break e.into(),
        };
        let avail = buffer_frames.saturating_sub(padding);
        if avail == 0 {
            continue;
        }
        let ptr = match unsafe { render.GetBuffer(avail) } {
            Ok(p) => p,
            Err(e) => break e.into(),
        };
        let out = unsafe { std::slice::from_raw_parts_mut(ptr as *mut f32, avail as usize * ENGINE_CHANNELS) };
        mixer.render(out);
        if let Err(e) = unsafe { render.ReleaseBuffer(avail, 0) } {
            break e.into();
        }
    };
    finish(&client, ended)
}
