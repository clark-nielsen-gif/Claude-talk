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
use windows::core::{implement, IUnknown, Interface, Ref, HRESULT};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, IAudioRenderClient, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{IAgileObject, IAgileObject_Impl, BLOB, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
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
    /// Not needed right now: the device is closed (capture streams still
    /// deliver real-time silence).
    Paused = 6,
}

impl StreamState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => StreamState::Running,
            2 => StreamState::WaitingForDevice,
            3 => StreamState::WaitingForProcess,
            6 => StreamState::Paused,
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
    /// Packets without a time from Windows: the timeline repair above can't
    /// work for them (shown in the diagnostics).
    pub untimed_packets: AtomicU64,
    pub silence_filled_frames: AtomicU64,
    /// Set by the control thread to make the stream reopen its device (e.g.
    /// the Windows default device changed).
    pub restart_requested: AtomicBool,
    /// Set by the control thread: keep the device closed until cleared.
    pub paused: AtomicBool,
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

    /// Closes the device (true) or opens it again (false).
    pub fn set_paused(&self, paused: bool) {
        if self.status.paused.swap(paused, Ordering::Relaxed) != paused {
            self.wake.set();
        }
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

/// The calling thread runs in Windows' "Pro Audio" class while this lives.
pub struct Mmcss(HANDLE);
impl Mmcss {
    pub fn enter() -> Option<Mmcss> {
        let mut idx = 0u32;
        unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut idx) }
            .ok()
            .map(Mmcss)
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
        if code == DEVICE_INVALIDATED
            || code == RESOURCES_INVALIDATED
            || code == SERVICE_NOT_RUNNING
            || code == NOT_FOUND
        {
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
    fn paused(&self) -> bool {
        self.status.paused.load(Ordering::Relaxed)
    }
    /// Sleeps up to `d`, waking early on stop/restart. Returns false on stop.
    fn sleep(&self, d: Duration) -> bool {
        unsafe { WaitForSingleObject(self.wake.0, d.as_millis() as u32) };
        !self.stopping()
    }
}

/// `idle` is called every ~10 ms while the stream waits between attempts.
/// Its argument is true when a new silent stretch begins (the previous run
/// had been delivering audio); consecutive failed attempts continue the same
/// stretch. Capture streams use it to keep delivering silence so the routes
/// downstream stay primed while a program or device is away.
fn supervise(
    name: &'static str,
    ctx: Ctx,
    mut run: impl FnMut(&Ctx) -> Ended + Send + 'static,
    mut idle: impl FnMut(bool) + Send + 'static,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("audio-{name}"))
        .spawn(move || {
            let _com = ComGuard::new();
            let _mmcss = Mmcss::enter();
            crate::flush_denormals();
            super::high_res_timer(); // the silence feed while waiting is clock-paced
            let mut backoff = Duration::from_millis(100);
            let mut first_wait = true;
            loop {
                if ctx.stopping() {
                    break;
                }
                if ctx.paused() {
                    // device closed; keep feeding `idle` (real-time silence)
                    ctx.status.set(StreamState::Paused);
                    let mut first = true;
                    while ctx.paused() {
                        idle(first);
                        first = false;
                        if !ctx.sleep(Duration::from_millis(10)) {
                            break;
                        }
                    }
                    if ctx.stopping() {
                        break;
                    }
                    // the device is opened fresh anyway
                    ctx.take_restart();
                    backoff = Duration::from_millis(100);
                }
                ctx.status.set(StreamState::Starting);
                let started = Instant::now();
                let ended = catch_unwind(AssertUnwindSafe(|| run(&ctx)))
                    .unwrap_or(Ended::Failed(HRESULT(-1)));
                let mut new_stretch = ctx.status.state() == StreamState::Running || first_wait;
                first_wait = false;
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
                // wait for `backoff`, in 10 ms steps, feeding `idle`
                let t0 = Instant::now();
                let mut stopped = false;
                while t0.elapsed() < backoff {
                    idle(new_stretch);
                    new_stretch = false;
                    if !ctx.sleep(Duration::from_millis(10)) {
                        stopped = true;
                        break;
                    }
                    if ctx.status.restart_requested.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if stopped {
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
    (
        Ctx {
            stop: stop.clone(),
            wake: wake.clone(),
            status: status.clone(),
        },
        stop,
        wake,
        status,
    )
}

/// Starts a capture stream feeding `fanout`.
pub fn spawn_capture(
    name: &'static str,
    target: CaptureTarget,
    fanout: SourceFanout,
) -> StreamHandle {
    let (ctx, stop, wake, status) = new_ctx();
    // The fanout is used by the run loop and, between runs, for silence.
    let shared = Arc::new(Mutex::new(fanout));
    let run_fanout = shared.clone();
    let mut silence_sent = 0u64;
    let mut idle_start: Option<Instant> = None;
    let thread = supervise(
        name,
        ctx,
        move |ctx| {
            let mut f = run_fanout.lock().unwrap_or_else(|p| p.into_inner());
            f.mark_restart();
            let ended = run_capture(ctx, &target, &mut f);
            // the gap until the silence feed takes over is not jitter either
            f.mark_restart();
            ended
        },
        move |new_stretch| {
            // real-time silence while waiting (one lock per 10 ms, not on a
            // running audio path)
            if new_stretch || idle_start.is_none() {
                // continue the timeline from the last real (or silent)
                // packet, not from now: the moments in between would
                // otherwise be a gap that grows the routes' buffers
                let last = shared.lock().map(|f| f.last_push_ns()).unwrap_or(0);
                let since = crate::mixer::clock_ns().saturating_sub(last);
                let back = if last == 0 { 0 } else { since.min(100_000_000) };
                idle_start = Some(Instant::now() - Duration::from_nanos(back));
                silence_sent = 0;
            }
            let start = idle_start.expect("set above");
            let due = (start.elapsed().as_secs_f64() * ENGINE_RATE as f64) as u64;
            let n = due
                .saturating_sub(silence_sent)
                .min(ENGINE_RATE as u64 / 10) as usize;
            if n > 0 {
                let mut f = shared.lock().unwrap_or_else(|p| p.into_inner());
                f.push(None, ENGINE_CHANNELS, n);
                silence_sent += n as u64;
            }
        },
    );
    StreamHandle {
        stop,
        wake,
        thread: Some(thread),
        status,
    }
}

/// Starts a render stream pulling from `mixer`.
pub fn spawn_render(
    name: &'static str,
    device: DeviceSelector,
    mut mixer: OutputMixer,
) -> StreamHandle {
    let (ctx, stop, wake, status) = new_ctx();
    let thread = supervise(
        name,
        ctx,
        move |ctx| run_render(ctx, &device, &mut mixer),
        |_: bool| {},
    );
    StreamHandle {
        stop,
        wake,
        thread: Some(thread),
        status,
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
/// Holds its own reference to the event: after a timeout the late callback
/// must still find a valid handle.
struct ActivationDone(Arc<Event>);

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationDone_Impl {
    fn ActivateCompleted(
        &self,
        _op: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe { SetEvent((self.0).0) }
    }
}

impl IAgileObject_Impl for ActivationDone_Impl {}

/// Opens an `IAudioClient` that records only `pid` and its children.
/// `wake` (stop or restart) ends the wait early: dropping a stream never
/// waits seconds for Windows here while the caller holds the audio lock.
fn activate_process_loopback(pid: u32, wake: HANDLE) -> windows::core::Result<IAudioClient> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    // ManuallyDrop: PROPVARIANT's Drop calls PropVariantClear, which would
    // try to free `params` (it lives on our stack).
    let mut pv = std::mem::ManuallyDrop::new(PROPVARIANT::default());
    unsafe {
        (*pv.Anonymous.Anonymous).vt = VT_BLOB;
        (*pv.Anonymous.Anonymous).Anonymous.blob = BLOB {
            cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: &params as *const _ as *mut u8,
        };
    }
    let done = Arc::new(Event::new()?);
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationDone(done.clone()).into();
    let op = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*pv),
            &handler,
        )
    }?;
    // the handler keeps its own reference to `done`: a late completion is safe
    if unsafe { WaitForMultipleObjects(&[done.0, wake], false, 5000) } != WAIT_OBJECT_0 {
        return Err(windows::Win32::Foundation::E_ABORT.into());
    }
    let mut hr = HRESULT(0);
    let mut unk: Option<IUnknown> = None;
    unsafe { op.GetActivateResult(&mut hr, &mut unk) }?;
    hr.ok()?;
    unk.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_NOINTERFACE))?
        .cast()
}

fn resolve_pid(target: &ProcessTarget) -> Option<u32> {
    match target {
        ProcessTarget::Pid(p) => Some(p.load(Ordering::Relaxed)).filter(|&p| p != 0),
        ProcessTarget::ExeNames(names) => process::find_roots(&process::snapshot(), names)
            .into_iter()
            .next(),
    }
}

fn run_capture(ctx: &Ctx, target: &CaptureTarget, fanout: &mut SourceFanout) -> Ended {
    // reopen everything if Windows' audio service restarts meanwhile
    let service_gen = super::service::generation();
    // ---- open (may allocate) ----
    let mut proc_handle: Option<ProcHandle> = None;
    let fill_silence;
    let client: IAudioClient = match target {
        CaptureTarget::Device(sel) | CaptureTarget::DeviceLoopback(sel) => {
            let (dev, info) = match devices::resolve(sel) {
                Ok(v) => v,
                Err(e) => return Ended::WaitDevice(e.code()),
            };
            *ctx.status
                .device_name
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = info.name;
            fill_silence = matches!(target, CaptureTarget::DeviceLoopback(_));
            match unsafe { dev.Activate::<IAudioClient>(CLSCTX_ALL, None) } {
                Ok(c) => c,
                Err(e) => return e.into(),
            }
        }
        CaptureTarget::Process(pt) => {
            let Some(pid) = resolve_pid(pt) else {
                return Ended::WaitProcess;
            };
            proc_handle = ProcHandle::open(pid);
            if proc_handle.is_none() {
                return Ended::WaitProcess;
            }
            ctx.status.pid.store(pid, Ordering::Relaxed);
            fill_silence = true;
            match activate_process_loopback(pid, ctx.wake.0) {
                Ok(c) => c,
                Err(e) => return Ended::Failed(e.code()),
            }
        }
    };
    let loopback = !matches!(target, CaptureTarget::Device(_));
    let mut flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if loopback {
        flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
    }
    let fmt = engine_format();
    // 100 ms of room: a late wake-up loses nothing (event mode, so packets
    // are still taken as soon as they arrive: no extra delay).
    if let Err(e) =
        unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &fmt, None) }
    {
        return e.into();
    }
    let event = match Event::new() {
        Ok(e) => e,
        Err(e) => return e.into(),
    };
    let capture: IAudioCaptureClient = match unsafe {
        client
            .SetEventHandle(event.0)
            .and_then(|_| client.GetService())
    } {
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
    // The timeline, in the packets' clock (100 ns QPC units): how far the
    // sound pushed so far reaches – packets and the silence put in for them.
    // Packet after packet, the timeline follows the sample count (the time
    // stamps can jitter by a fraction of a millisecond), pulled 5 % towards
    // the stamps so it follows the clock. Only after silence was put in, or
    // when a stamp jumps by 3 ms or more (packets lost), are stamps compared:
    // a late packet's overlap with silence already sent is dropped, and a
    // gap after lost packets is filled with silence. So the timeline (and
    // the echo reference's delay) does not move.
    let mut pos = qpc_100ns() as f64;
    // pos comes from a packet or from silence (not just the start time)
    let mut timed = false;
    // the last thing pushed was silence that may overlap the next packet
    let mut after_silence = false;
    // silence fed since the program went quiet: (from, frames sent), QPC
    let mut quiet: Option<(f64, u64)> = None;
    // the last sample pushed, to fade out into silence without a click
    let mut last_sample = [0f32; ENGINE_CHANNELS];
    // A quiet program sends no packets; look every 5 ms so the silence that
    // replaces them is on time (late silence would make the routes underrun
    // and grow their buffers).
    let timeout = if fill_silence { 5 } else { 30 };
    let ended = loop {
        let w = unsafe { WaitForMultipleObjects(&handles[..n_handles], false, timeout) };
        if ctx.stopping() {
            break Ended::Stop;
        }
        if ctx.take_restart() || ctx.paused() || super::service::current() != service_gen {
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
            let mut qpc = 0u64;
            if let Err(e) = unsafe {
                capture.GetBuffer(&mut data, &mut frames, &mut bflags, None, Some(&mut qpc))
            } {
                return finish(&client, e.into());
            }
            if bflags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                ctx.status.discontinuities.fetch_add(1, Ordering::Relaxed);
            }
            if qpc == 0 {
                ctx.status.untimed_packets.fetch_add(1, Ordering::Relaxed);
            }
            let to_frames = |d: f64| (d * ENGINE_RATE as f64 / 1e7) as u64;
            let stamp = qpc as f64;
            let dur = frames as f64 * 1e7 / ENGINE_RATE as f64;
            let contiguous =
                timed && !after_silence && (qpc == 0 || (stamp - pos).abs() < 30_000.0);
            let compare = fill_silence && timed && qpc != 0 && !contiguous;
            // part of this packet was already played as silence (it came
            // late); small differences (< 0.5 ms) are clock rounding
            let skip = if compare && pos > stamp + 5_000.0 {
                (to_frames(pos - stamp) as u32).min(frames)
            } else {
                0
            };
            // packets were lost before this one: the time passed all the same
            let gap = if compare && stamp > pos + 10_000.0 {
                to_frames(stamp - pos).min(ENGINE_RATE as u64 / 10) as usize
            } else {
                0
            };
            if gap > 0 {
                push_silence_after(fanout, &mut last_sample, gap);
                ctx.status
                    .silence_filled_frames
                    .fetch_add(gap as u64, Ordering::Relaxed);
            }
            let keep = (frames - skip) as usize;
            if keep == 0 {
                // entirely covered by the silence already sent
            } else if bflags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                fanout.push(None, ENGINE_CHANNELS, keep);
                last_sample = [0.0; ENGINE_CHANNELS];
            } else {
                let samples = unsafe {
                    std::slice::from_raw_parts(
                        data as *const f32,
                        frames as usize * ENGINE_CHANNELS,
                    )
                };
                let kept = &samples[skip as usize * ENGINE_CHANNELS..];
                last_sample.copy_from_slice(&kept[(keep - 1) * ENGINE_CHANNELS..]);
                if skip > 0 || gap > 0 {
                    // sound after silence starts mid-packet: fade it in over
                    // 1 ms instead of a hard cut (a click)
                    const FADE: usize = 48;
                    let n = keep.min(FADE);
                    let mut head = [0f32; FADE * ENGINE_CHANNELS];
                    for f in 0..n {
                        let g = (f + 1) as f32 / (n + 1) as f32;
                        for c in 0..ENGINE_CHANNELS {
                            head[f * ENGINE_CHANNELS + c] = kept[f * ENGINE_CHANNELS + c] * g;
                        }
                    }
                    fanout.push(Some(&head[..n * ENGINE_CHANNELS]), ENGINE_CHANNELS, n);
                    if keep > n {
                        fanout.push(
                            Some(&kept[n * ENGINE_CHANNELS..]),
                            ENGINE_CHANNELS,
                            keep - n,
                        );
                    }
                } else {
                    fanout.push(Some(kept), ENGINE_CHANNELS, keep);
                }
            }
            if contiguous {
                let predicted = pos + dur;
                pos = if qpc != 0 {
                    predicted + 0.05 * (stamp + dur - predicted)
                } else {
                    predicted
                };
                after_silence = false;
            } else {
                let end = if qpc != 0 {
                    stamp + dur
                } else {
                    qpc_100ns() as f64
                };
                // A packet entirely covered by silence already sent ends
                // before `pos`: the timeline stays where the silence reached,
                // and the next packet is compared with it too.
                after_silence = timed && pos > end;
                pos = if timed { pos.max(end) } else { end };
            }
            timed = true;
            if fill_silence {
                fanout.set_quiet_fill(false);
                set_reach(fanout, pos);
            }
            if let Err(e) = unsafe { capture.ReleaseBuffer(frames) } {
                return finish(&client, e.into());
            }
            got = true;
        }
        let now = Instant::now();
        if got {
            last_data = now;
            quiet = None;
        } else if fill_silence && now.duration_since(last_data) >= Duration::from_millis(15) {
            // Loopback streams deliver nothing while the program is silent.
            // Keep the downstream buffers moving with silence, sample for
            // sample in real time from the last packet, so mixing continues
            // and drift control stays locked. The silence reaches up to one
            // packet (10 ms) before now: a packet that arrives now covers
            // the time up to then, and its start is not cut off.
            let now_qpc = qpc_100ns() as f64 - 100_000.0;
            let (since, sent) = quiet.get_or_insert((pos, 0));
            let due = ((now_qpc - *since).max(0.0) * ENGINE_RATE as f64 / 1e7) as u64;
            let frames = due.saturating_sub(*sent).min(ENGINE_RATE as u64 / 10) as usize;
            if frames > 0 {
                // flagged before the silence arrives at the routes
                fanout.set_quiet_fill(true);
                push_silence_after(fanout, &mut last_sample, frames);
                *sent += frames as u64;
                // exactly as far as the silence sent reaches
                pos = *since + *sent as f64 * 1e7 / ENGINE_RATE as f64;
                timed = true;
                after_silence = true;
                fanout.set_quiet_fill(true);
                // The silence reaches 10 ms before now: say so, or the echo
                // reference's delay control would read the delay 10 ms short
                // and move the reference later for every pause.
                set_reach(fanout, pos);
                ctx.status
                    .silence_filled_frames
                    .fetch_add(frames as u64, Ordering::Relaxed);
            }
        }
    };
    finish(&client, ended)
}

/// Tells the routes where the pushed sound reaches (`pos`, QPC 100 ns), on
/// the mixer's clock.
fn set_reach(fanout: &crate::mixer::SourceFanout, pos: f64) {
    let behind_ns = ((qpc_100ns() as f64 - pos).max(0.0) * 100.0) as u64;
    fanout.set_reach_ns(crate::mixer::clock_ns().saturating_sub(behind_ns));
}

/// Silence after sound: the first millisecond fades out from the last
/// sample pushed instead of cutting to zero (a click on every route).
fn push_silence_after(
    fanout: &mut crate::mixer::SourceFanout,
    last: &mut [f32; ENGINE_CHANNELS],
    frames: usize,
) {
    const FADE: usize = 48;
    let n = frames.min(FADE);
    if last.iter().any(|v| *v != 0.0) && n > 0 {
        let mut ramp = [0f32; FADE * ENGINE_CHANNELS];
        for f in 0..n {
            let g = 1.0 - (f + 1) as f32 / (n + 1) as f32;
            for c in 0..ENGINE_CHANNELS {
                ramp[f * ENGINE_CHANNELS + c] = last[c] * g;
            }
        }
        fanout.push(Some(&ramp[..n * ENGINE_CHANNELS]), ENGINE_CHANNELS, n);
        if frames > n {
            fanout.push(None, ENGINE_CHANNELS, frames - n);
        }
    } else {
        fanout.push(None, ENGINE_CHANNELS, frames);
    }
    *last = [0.0; ENGINE_CHANNELS];
}

/// Now on the clock WASAPI uses for packet positions (100 ns units).
fn qpc_100ns() -> u64 {
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut c, mut f) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
    }
    if f <= 0 {
        return 0;
    }
    (c as i128 * 10_000_000 / f as i128) as u64
}

fn finish(client: &IAudioClient, ended: Ended) -> Ended {
    unsafe {
        let _ = client.Stop();
    }
    ended
}

fn run_render(ctx: &Ctx, sel: &DeviceSelector, mixer: &mut OutputMixer) -> Ended {
    // reopen everything if Windows' audio service restarts meanwhile
    let service_gen = super::service::generation();
    // ---- open ----
    let (dev, info) = match devices::resolve(sel) {
        Ok(v) => v,
        Err(e) => return Ended::WaitDevice(e.code()),
    };
    *ctx.status
        .device_name
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = info.name;
    let client: IAudioClient = match unsafe { dev.Activate(CLSCTX_ALL, None) } {
        Ok(c) => c,
        Err(e) => return e.into(),
    };
    let fmt = engine_format();
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if let Err(e) = unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, &fmt, None) }
    {
        return e.into();
    }
    let event = match Event::new() {
        Ok(e) => e,
        Err(e) => return e.into(),
    };
    let (render, buffer_frames): (IAudioRenderClient, u32) = match unsafe {
        client
            .SetEventHandle(event.0)
            .and_then(|_| Ok((client.GetService()?, client.GetBufferSize()?)))
    } {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    // Keep 1.5 device periods queued, not the whole buffer (shared-mode
    // buffers are often ~22 ms; a full one would add that as latency).
    let mut period_hns = 0i64;
    let _ = unsafe { client.GetDevicePeriod(Some(&mut period_hns), None) };
    let period_frames = ((period_hns.max(30_000) as u64 * ENGINE_RATE as u64) / 10_000_000) as u32;
    let queue_frames = (period_frames + period_frames / 2).min(buffer_frames);
    ctx.status
        .period_frames
        .store(queue_frames, Ordering::Relaxed);
    // pre-roll the queue with silence so the first period never underruns
    unsafe {
        if render.GetBuffer(queue_frames).is_ok() {
            let _ = render.ReleaseBuffer(queue_frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
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
        if ctx.take_restart() || ctx.paused() || super::service::current() != service_gen {
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
        let avail = queue_frames.saturating_sub(padding);
        if avail == 0 {
            continue;
        }
        let ptr = match unsafe { render.GetBuffer(avail) } {
            Ok(p) => p,
            Err(e) => break e.into(),
        };
        let out = unsafe {
            std::slice::from_raw_parts_mut(ptr as *mut f32, avail as usize * ENGINE_CHANNELS)
        };
        mixer.render(out);
        if let Err(e) = unsafe { render.ReleaseBuffer(avail, 0) } {
            break e.into();
        }
    };
    finish(&client, ended)
}
