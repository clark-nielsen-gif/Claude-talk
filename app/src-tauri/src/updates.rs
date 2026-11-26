//! Automatic updates from GitHub Releases. Only active when the release was
//! built with an update-signing key (see SIGNING.md): the public key is baked
//! in at build time, and every update manifest must be signed with it.

use tauri::AppHandle;

#[cfg_attr(not(feature = "updater"), allow(dead_code))]
pub const ENDPOINT: &str =
    "https://github.com/clark-nielsen-gif/Claude-talk/releases/latest/download/latest.json";

#[cfg_attr(not(feature = "updater"), allow(dead_code))]
pub fn pubkey() -> Option<&'static str> {
    option_env!("CLAUDE_TALK_UPDATER_PUBKEY").filter(|k| !k.trim().is_empty())
}

#[cfg(feature = "updater")]
pub fn check_in_background(app: AppHandle) {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let Some(key) = pubkey() else {
        log::info!("updates: not configured in this build");
        return;
    };
    tauri::async_runtime::spawn(async move {
        let Ok(url) = ENDPOINT.parse() else { return };
        let updater = match app
            .updater_builder()
            .pubkey(key)
            .endpoints(vec![url])
            .and_then(|b| b.build())
        {
            Ok(u) => u,
            Err(e) => return log::warn!("updates: {e}"),
        };
        match updater.check().await {
            Ok(Some(u)) => {
                log::info!("updates: version {} available", u.version);
                let _ = app.emit("update-available", u.version.clone());
            }
            Ok(None) => log::info!("updates: up to date"),
            Err(e) => log::info!("updates: check failed ({e})"),
        }
    });
}

#[cfg(not(feature = "updater"))]
pub fn check_in_background(_app: AppHandle) {}

/// Downloads and installs the update; the installer restarts Claude Talk.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "updater")]
    {
        use tauri_plugin_updater::UpdaterExt;
        let key = pubkey().ok_or("error.internal")?;
        let url = ENDPOINT.parse().map_err(|_| "error.internal")?;
        let updater = app
            .updater_builder()
            .pubkey(key)
            .endpoints(vec![url])
            .and_then(|b| b.build())
            .map_err(|_| "error.update")?;
        let Some(update) = updater.check().await.map_err(|_| "error.update")? else {
            return Ok(());
        };
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| {
                log::warn!("updates: install failed: {e}");
                "error.update".to_string()
            })?;
        app.restart();
    }
    #[cfg(not(feature = "updater"))]
    {
        let _ = app;
        Err("error.update".into())
    }
}
