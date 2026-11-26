//! Notices when Windows' audio service restarts. Streams opened before that
//! keep "running" without sound and without an error, so each stream compares
//! the restart counter (one atomic read, real-time safe) and reopens.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Once;
use std::time::Duration;
use windows::core::w;
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_MANAGER_CONNECT,
    SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS, SERVICE_STATUS_PROCESS,
};

static GENERATION: AtomicU32 = AtomicU32::new(0);

/// How many times the audio service has restarted since we started
/// watching. Starts the watcher on first use (call it when opening a stream).
pub fn generation() -> u32 {
    static WATCH: Once = Once::new();
    WATCH.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("audio-service-watch".into())
            .spawn(|| {
                let mut last = audio_service_pid();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    let now = audio_service_pid();
                    if now != 0 && last != 0 && now != last {
                        GENERATION.fetch_add(1, Ordering::Relaxed);
                    }
                    if now != 0 {
                        last = now;
                    }
                }
            });
    });
    GENERATION.load(Ordering::Relaxed)
}

/// The counter without starting the watcher (for the audio threads).
pub fn current() -> u32 {
    GENERATION.load(Ordering::Relaxed)
}

/// Process id of the service host running Audiosrv (0 if stopped/unknown).
fn audio_service_pid() -> u32 {
    unsafe {
        let Ok(scm) = OpenSCManagerW(None, None, SC_MANAGER_CONNECT) else {
            return 0;
        };
        let mut pid = 0;
        if let Ok(svc) = OpenServiceW(scm, w!("Audiosrv"), SERVICE_QUERY_STATUS) {
            let mut st = SERVICE_STATUS_PROCESS::default();
            let mut needed = 0u32;
            let buf = std::slice::from_raw_parts_mut(
                &mut st as *mut _ as *mut u8,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
            );
            if QueryServiceStatusEx(svc, SC_STATUS_PROCESS_INFO, Some(buf), &mut needed).is_ok() {
                pid = st.dwProcessId;
            }
            let _ = CloseServiceHandle(svc);
        }
        let _ = CloseServiceHandle(scm);
        pid
    }
}
