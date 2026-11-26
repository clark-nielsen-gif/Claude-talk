//! Finding the processes whose sound we capture (Discord, the Claude
//! window's WebView2 browser process, test tone players).

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
}

/// Snapshot of all processes.
pub fn snapshot() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                out.push(ProcInfo {
                    pid: e.th32ProcessID,
                    parent: e.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase(),
                });
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Root processes among those named `names` (case-insensitive): processes
/// whose parent is not itself one of the named processes. For Chromium/
/// Electron apps (Discord) this is the browser process that owns the whole
/// tree, including the audio service child.
pub fn find_roots(procs: &[ProcInfo], names: &[String]) -> Vec<u32> {
    let names: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let matching: Vec<&ProcInfo> = procs.iter().filter(|p| names.contains(&p.exe)).collect();
    let ids: std::collections::HashSet<u32> = matching.iter().map(|p| p.pid).collect();
    let mut roots: Vec<u32> = matching.iter().filter(|p| !ids.contains(&p.parent)).map(|p| p.pid).collect();
    roots.sort_unstable();
    roots
}

/// Owned process handle that can be waited on.
pub struct ProcHandle(pub HANDLE);

impl ProcHandle {
    pub fn open(pid: u32) -> Option<ProcHandle> {
        unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok().map(ProcHandle)
    }
}

impl Drop for ProcHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_skip_children_of_the_same_app() {
        let p = |pid, parent, exe: &str| ProcInfo { pid, parent, exe: exe.into() };
        let procs = vec![
            p(10, 1, "update.exe"),
            p(11, 10, "discord.exe"),
            p(12, 11, "discord.exe"),
            p(13, 11, "discord.exe"),
            p(20, 1, "chrome.exe"),
        ];
        assert_eq!(find_roots(&procs, &["Discord.exe".into(), "DiscordPTB.exe".into()]), vec![11]);
        assert_eq!(find_roots(&procs, &["slack.exe".into()]), Vec::<u32>::new());
    }
}
