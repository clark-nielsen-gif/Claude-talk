//! Windows audio back-end: WASAPI capture, render and process loopback.
//!
//! Every stream runs on its own thread with the MMCSS "Pro Audio" class. A
//! stream thread is a *supervisor*: it opens the device or process, runs the
//! event loop until something goes wrong (device unplugged, default device
//! changed, process exited, sample rate changed, sleep/resume), then closes
//! everything and tries again with back-off. It never panics into the app;
//! failures are reported through [`StreamStatus`].

pub mod devices;
pub mod host;
pub mod policy;
pub mod process;
pub mod sessions;
pub mod stream;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// Initialises COM (multi-threaded) for the current thread for its lifetime.
pub struct ComGuard(bool);

impl ComGuard {
    pub fn new() -> Self {
        // S_FALSE (already initialised) is fine; RPC_E_CHANGED_MODE means the
        // thread is STA already, which also works for our calls.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ComGuard(hr.is_ok())
    }
}

impl Default for ComGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// Windows build number (e.g. 22631), read from the kernel.
pub fn windows_build() -> u32 {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    let mut info = OSVERSIONINFOW { dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    let _ = unsafe { RtlGetVersion(&mut info) };
    info.dwBuildNumber
}

/// Process loopback that includes a whole process tree needs build 20348+
/// (Windows 11 or Windows Server 2022).
pub fn supports_process_loopback() -> bool {
    windows_build() >= 20348
}
