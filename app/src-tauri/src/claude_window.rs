//! The Claude window: claude.ai in its own WebView2 environment.
//!
//! * Own user-data folder → own browser process tree, so Claude's voice can
//!   be captured with process loopback without catching anything else, and
//!   the login is remembered between starts.
//! * `bridge.js` replaces the page's microphone with the engine's mix.
//! * Microphone permission is granted automatically, and only to Claude's
//!   own domains. Every other site opens in the user's normal browser.
//! * The app never reads cookies, passwords or codes: login happens on
//!   Anthropic's pages inside the WebView; we only look at URLs to decide
//!   where a navigation may go.

use crate::state::AppState;
use base64::Engine as _;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "claude";
const BRIDGE_JS: &str = include_str!("claude/bridge.js");
const LOGIN_HELP_JS: &str = include_str!("claude/login_help.js");

/// Where the Claude window starts.
fn start_url() -> Url {
    #[cfg(feature = "ci-testpage")]
    if let Ok(u) = std::env::var("CLAUDE_TALK_TEST_URL") {
        if let Ok(url) = Url::parse(&u) {
            return url;
        }
    }
    Url::parse("https://claude.ai/new").expect("static url")
}

/// Hosts that belong to Claude's sign-in and app (see DECISIONS.md D7).
pub fn is_claude_host(url: &Url) -> bool {
    #[cfg(feature = "ci-testpage")]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ["claude.ai", "anthropic.com", "claude.com"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        || host == "challenges.cloudflare.com"
}

fn is_google_login(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("accounts.google.com") | Some("accounts.youtube.com")
    )
}

/// What to do with a navigation in the Claude window.
#[derive(Debug, PartialEq, Eq)]
pub enum NavDecision {
    Allow,
    /// Google blocks sign-in inside apps; show our guide instead.
    GoogleLoginHelp,
    OpenInBrowser,
    Ignore,
}

pub fn decide(url: &Url) -> NavDecision {
    match url.scheme() {
        "about" | "data" | "blob" => return NavDecision::Allow,
        "http" | "https" => {}
        _ => return NavDecision::Ignore, // e.g. custom app protocols
    }
    if is_claude_host(url) {
        NavDecision::Allow
    } else if is_google_login(url) {
        NavDecision::GoogleLoginHelp
    } else {
        NavDecision::OpenInBrowser
    }
}

fn open_external(app: &AppHandle, url: &Url) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_url(url.as_str(), None::<&str>);
}

fn show_login_help(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let lang = app
            .state::<AppState>()
            .config
            .lock()
            .map(|c| c.language.clone())
            .unwrap_or_default();
        let _ = w.eval(format!(
            "{LOGIN_HELP_JS}\n;window.__claudeTalkLoginHelp && window.__claudeTalkLoginHelp({});",
            serde_json::json!(lang)
        ));
    }
}

/// Opens (or focuses) the Claude window.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let data_dir = crate::paths::claude_window_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let nav_app = app.clone();
    let popup_app = app.clone();
    #[allow(unused_mut)]
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(start_url()));
    // CI builds only: the voice test page plays and records without a click.
    #[cfg(feature = "ci-testpage")]
    {
        builder = builder.additional_browser_args(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required",
        );
    }
    let window = builder
        .title("Claude")
        .inner_size(1100.0, 820.0)
        .min_inner_size(520.0, 520.0)
        .data_directory(data_dir)
        .initialization_script(BRIDGE_JS)
        .on_navigation(move |url| match decide(url) {
            NavDecision::Allow => true,
            NavDecision::GoogleLoginHelp => {
                show_login_help(&nav_app);
                false
            }
            NavDecision::OpenInBrowser => {
                open_external(&nav_app, url);
                false
            }
            NavDecision::Ignore => false,
        })
        .on_new_window(move |url, _features| {
            match decide(&url) {
                NavDecision::GoogleLoginHelp => show_login_help(&popup_app),
                NavDecision::Ignore => {}
                // Pop-ups (also Claude's own links to docs etc.) go to the
                // normal browser; the Claude window stays one page.
                _ => open_external(&popup_app, &url),
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .build()?;
    #[cfg(windows)]
    win::attach(app.clone(), &window);
    let _ = window.set_focus();
    Ok(())
}

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

/// Hands the instructions to the page script, which writes them into
/// Claude's message field and sends them (nothing on the page is read).
pub fn send_prompt(app: &AppHandle, text: &str) {
    let msg = format!("ct:p:{}", serde_json::Value::String(text.to_string()));
    #[cfg(windows)]
    {
        let _ = app.run_on_main_thread(move || win::post(&msg));
    }
    #[cfg(not(windows))]
    let _ = (app, msg);
}

/// Sends the instructions automatically when a new conversation starts.
fn auto_send(app: &AppHandle) {
    let text = {
        let st = app.state::<AppState>();
        let c = st.config.lock().unwrap_or_else(|p| p.into_inner());
        if !c.instructions.auto_send {
            return;
        }
        c.instructions.text(&c.language)
    };
    if !text.is_empty() {
        log::info!("Claude window: sending the instructions to a new conversation");
        send_prompt(app, &text);
    }
}

/// Sends the engine's "Til Claude" mix to the Claude window, 10 ms at a time.
///
/// Two threads: the clock thread is the virtual microphone's clock (Pro
/// Audio priority, no allocation) and only mixes into a lock-free ring; the
/// sender thread encodes the blocks and hands them to the window, with at
/// most a few posts in flight so a busy UI thread cannot pile them up.
pub fn spawn_bridge(app: AppHandle, mut mixer: ct_engine::mixer::OutputMixer) {
    const BLOCK: usize = 480;
    const MAX_IN_FLIGHT: u32 = 4;
    // 200 ms of room; the sender keeps it nearly empty
    let (mut tx, mut rx) = rtrb::RingBuffer::<f32>::new(BLOCK * 20);
    let state = app.state::<AppState>().inner().clone();
    let clock_state = state.clone();
    std::thread::Builder::new()
        .name("claude-bridge-clock".into())
        .spawn(move || {
            #[cfg(windows)]
            let _period = win::TimerResolution::new();
            #[cfg(windows)]
            let _mmcss = ct_engine::win::stream::Mmcss::enter();
            ct_engine::flush_denormals();
            let mut buf = vec![0f32; BLOCK];
            let start = Instant::now();
            let mut sent: u64 = 0;
            while !clock_state.shutting_down.load(Ordering::Relaxed) {
                let due = (start.elapsed().as_secs_f64() * 48_000.0) as u64;
                if due < sent + BLOCK as u64 {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                // After a stall of more than 30 ms, skip ahead instead of
                // catching up: rendering several blocks at once would empty
                // the route buffers, and their targets would grow for good.
                if due > sent + 3 * BLOCK as u64 {
                    sent = due - BLOCK as u64;
                }
                mixer.render(&mut buf);
                sent += BLOCK as u64;
                if let Ok(mut chunk) = tx.write_chunk(BLOCK) {
                    let (a, b) = chunk.as_mut_slices();
                    a.copy_from_slice(&buf[..a.len()]);
                    b.copy_from_slice(&buf[a.len()..]);
                    chunk.commit_all();
                }
            }
        })
        .expect("spawn bridge clock");
    std::thread::Builder::new()
        .name("claude-bridge".into())
        .spawn(move || {
            #[cfg(windows)]
            let _period = win::TimerResolution::new();
            let in_flight = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let mut block = vec![0f32; BLOCK];
            let mut bytes = vec![0u8; BLOCK * 4];
            let mut text = String::with_capacity(8 + BLOCK * 4 * 4 / 3 + 4);
            while !state.shutting_down.load(Ordering::Relaxed) {
                let Ok(chunk) = rx.read_chunk(BLOCK) else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                let (a, b) = chunk.as_slices();
                block[..a.len()].copy_from_slice(a);
                block[a.len()..].copy_from_slice(b);
                chunk.commit_all();
                if let Ok(mut probe) = state.bridge_probe.try_lock() {
                    if let Some(v) = probe.as_mut() {
                        v.extend_from_slice(&block);
                    }
                }
                if !state.claude_mic_active.load(Ordering::Relaxed) {
                    continue;
                }
                if in_flight.load(Ordering::Relaxed) >= MAX_IN_FLIGHT {
                    continue; // the page side covers a dropped block with silence
                }
                for (d, v) in bytes.chunks_exact_mut(4).zip(&block) {
                    d.copy_from_slice(&v.to_le_bytes());
                }
                text.clear();
                text.push_str("ct:a:");
                base64::engine::general_purpose::STANDARD.encode_string(&bytes, &mut text);
                #[cfg(windows)]
                {
                    let msg = text.clone();
                    let counter = in_flight.clone();
                    counter.fetch_add(1, Ordering::Relaxed);
                    if app
                        .run_on_main_thread(move || {
                            win::post(&msg);
                            counter.fetch_sub(1, Ordering::Relaxed);
                        })
                        .is_err()
                    {
                        in_flight.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }
        })
        .expect("spawn bridge");
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::cell::RefCell;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    use webview2_com::{PermissionRequestedEventHandler, WebMessageReceivedEventHandler};
    use windows::core::{HSTRING, PWSTR};

    thread_local! {
        // ICoreWebView2 is apartment-bound: kept on the UI thread only.
        static CORE: RefCell<Option<ICoreWebView2>> = const { RefCell::new(None) };
    }

    /// Raises the system timer resolution to 1 ms while alive (bridge clock).
    pub struct TimerResolution;
    impl TimerResolution {
        pub fn new() -> Self {
            unsafe { windows::Win32::Media::timeBeginPeriod(1) };
            TimerResolution
        }
    }
    impl Drop for TimerResolution {
        fn drop(&mut self) {
            unsafe { windows::Win32::Media::timeEndPeriod(1) };
        }
    }

    pub fn post(msg: &str) {
        CORE.with(|c| {
            if let Some(core) = c.borrow().as_ref() {
                let _ = unsafe { core.PostWebMessageAsString(&HSTRING::from(msg)) };
            }
        });
    }

    fn take(p: PWSTR) -> String {
        webview2_com::take_pwstr(p)
    }

    fn uri_allowed(uri: &str) -> bool {
        Url::parse(uri).map(|u| is_claude_host(&u)).unwrap_or(false)
    }

    pub fn attach(app: AppHandle, window: &tauri::WebviewWindow) {
        let app2 = app.clone();
        let _ = window.with_webview(move |pw| unsafe {
            let Ok(core) = pw.controller().CoreWebView2() else {
                return;
            };
            // The browser process of *this* WebView2 environment: process
            // loopback on it captures only Claude's voice.
            let mut pid = 0u32;
            if core.BrowserProcessId(&mut pid).is_ok() {
                app2.state::<AppState>()
                    .claude_pid
                    .store(pid, Ordering::Relaxed);
                log::info!("Claude window browser process: {pid}");
            }
            let mut token = 0i64;
            let _ = core.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri = take(uri);
                    let mut kind = Default::default();
                    args.PermissionKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                        let state = if uri_allowed(&uri) {
                            COREWEBVIEW2_PERMISSION_STATE_ALLOW
                        } else {
                            COREWEBVIEW2_PERMISSION_STATE_DENY
                        };
                        args.SetState(state)?;
                    }
                    Ok(())
                })),
                &mut token,
            );
            let msg_app = app2.clone();
            let _ = core.add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut src = PWSTR::null();
                    args.Source(&mut src)?;
                    if !uri_allowed(&take(src)) {
                        return Ok(());
                    }
                    let mut s = PWSTR::null();
                    if args.TryGetWebMessageAsString(&mut s).is_err() {
                        return Ok(());
                    }
                    crate::claude_window::on_page_message(&msg_app, &take(s));
                    Ok(())
                })),
                &mut token,
            );
            // For error reports: did the page load (host only, never the
            // full address), and did a WebView2 process fail?
            let _ = core.add_NavigationCompleted(
                &webview2_com::NavigationCompletedEventHandler::create(Box::new(|wv, args| {
                    let (Some(wv), Some(args)) = (wv, args) else {
                        return Ok(());
                    };
                    let mut ok = windows::core::BOOL::default();
                    args.IsSuccess(&mut ok)?;
                    let mut status = Default::default();
                    args.WebErrorStatus(&mut status)?;
                    let mut src = PWSTR::null();
                    wv.Source(&mut src)?;
                    let host = Url::parse(&take(src))
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_string))
                        .unwrap_or_default();
                    if ok.as_bool() {
                        log::info!("Claude window loaded {host}");
                    } else {
                        log::warn!(
                            "Claude window could not load {host} (web error {})",
                            status.0
                        );
                    }
                    Ok(())
                })),
                &mut token,
            );
            let fail_app = app2.clone();
            let _ = core.add_ProcessFailed(
                &webview2_com::ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        let mut kind = Default::default();
                        args.ProcessFailedKind(&mut kind)?;
                        log::warn!("Claude window: a WebView2 process failed (kind {})", kind.0);
                        // The browser process is gone: forget its id so a new
                        // program that gets the same id is never recorded.
                        if kind.0 == 0 {
                            let st = fail_app.state::<AppState>();
                            st.claude_pid.store(0, Ordering::Relaxed);
                            st.claude_mic_active.store(false, Ordering::Relaxed);
                        }
                    }
                    Ok(())
                })),
                &mut token,
            );
            CORE.with(|c| *c.borrow_mut() = Some(core));
        });
        let app3 = app.clone();
        window.on_window_event(move |e| {
            if let tauri::WindowEvent::Destroyed = e {
                CORE.with(|c| *c.borrow_mut() = None);
                let st = app3.state::<AppState>();
                st.claude_pid.store(0, Ordering::Relaxed);
                st.claude_mic_active.store(false, Ordering::Relaxed);
                let _ = app3.emit("claude-window", serde_json::json!({"open": false}));
            }
        });
    }
}

/// Messages from bridge.js (JSON with `ct: 1`).
pub fn on_page_message(app: &AppHandle, text: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if v.get("ct").and_then(|x| x.as_i64()) != Some(1) {
        return;
    }
    let state: Arc<crate::state::Inner> = app.state::<AppState>().inner().clone();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("mic") => {
            let active = v.get("active").and_then(|a| a.as_bool()).unwrap_or(false);
            state.claude_mic_active.store(active, Ordering::Relaxed);
            crate::audio::refresh_mic(app);
            log::info!(
                "Claude window microphone {}",
                if active { "in use" } else { "released" }
            );
            let _ = app.emit(
                "claude-window",
                serde_json::json!({"open": true, "micActive": active}),
            );
        }
        Some("fresh") => auto_send(app),
        Some("prompt") => {
            let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
            let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("");
            log::info!(
                "Claude window: instructions {}",
                if ok { "sent" } else { "not sent" }
            );
            let _ = app.emit(
                "claude-prompt",
                serde_json::json!({ "ok": ok, "reason": reason }),
            );
        }
        Some("ready") => {
            // a new page: it has not asked for the microphone yet
            state.claude_mic_active.store(false, Ordering::Relaxed);
            crate::audio::refresh_mic(app);
            let _ = app.emit("claude-window", serde_json::json!({"open": true}));
        }
        Some("source") => log::info!(
            "bridge source mode: {}",
            v.get("mode").and_then(|m| m.as_str()).unwrap_or("?")
        ),
        Some("stats") => log::debug!("bridge stats: {v}"),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_rules() {
        let d = |s: &str| decide(&Url::parse(s).unwrap());
        assert_eq!(d("https://claude.ai/new"), NavDecision::Allow);
        assert_eq!(d("https://login.anthropic.com/x"), NavDecision::Allow);
        assert_eq!(
            d("https://challenges.cloudflare.com/cdn-cgi"),
            NavDecision::Allow
        );
        assert_eq!(
            d("https://accounts.google.com/o/oauth2/auth"),
            NavDecision::GoogleLoginHelp
        );
        assert_eq!(d("https://example.com/"), NavDecision::OpenInBrowser);
        assert_eq!(d("https://claude.ai.evil.com/"), NavDecision::OpenInBrowser);
        assert_eq!(d("https://evilclaude.ai/"), NavDecision::OpenInBrowser);
        assert_eq!(d("http://claude.ai/"), NavDecision::OpenInBrowser);
        assert_eq!(d("ms-settings:privacy"), NavDecision::Ignore);
        #[cfg(not(feature = "ci-testpage"))]
        assert_eq!(d("http://127.0.0.1:8123/test"), NavDecision::OpenInBrowser);
    }
}
