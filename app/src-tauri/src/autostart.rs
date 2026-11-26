//! "Start with Windows" (on by default, can be turned off).
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub fn apply(app: &AppHandle, enabled: bool) {
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    if let Err(e) = r {
        log::warn!(
            "autostart {}: {e}",
            if enabled { "enable" } else { "disable" }
        );
    }
}
