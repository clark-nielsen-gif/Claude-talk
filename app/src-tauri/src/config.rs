//! Settings in `%APPDATA%\Claude Talk\config.json`, versioned so later
//! releases can migrate old files.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    pub mute_claude_to_discord: String,
    pub mute_mic: String,
    pub push_to_talk_claude: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            // Function keys: Ctrl+Alt+letter is AltGr+letter on Danish and
            // many other keyboards (Ctrl+Alt+M types µ).
            mute_claude_to_discord: "Ctrl+Shift+F9".into(),
            mute_mic: "Ctrl+Shift+F10".into(),
            push_to_talk_claude: "Ctrl+Shift+F12".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EchoMode {
    /// On automatically when speakers are detected.
    Auto,
    On,
    Off,
}

/// "Instruktioner til Claude": how Claude behaves in the call. Sent to the
/// Claude window as the first message of a new conversation (or by hand).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Instructions {
    /// Answer only when someone says the wake word.
    pub only_when_called: bool,
    pub wake_word: String,
    pub short: bool,
    pub danish: bool,
    /// Remember that several people talk.
    pub group: bool,
    /// The user's own text.
    pub own: String,
    /// Send automatically when a new conversation starts.
    pub auto_send: bool,
}

impl Default for Instructions {
    fn default() -> Self {
        Instructions {
            only_when_called: false,
            wake_word: "Claude".into(),
            short: false,
            danish: false,
            group: true,
            own: String::new(),
            auto_send: true,
        }
    }
}

impl Instructions {
    /// The message Claude gets, in the app's language; empty if nothing is chosen.
    pub fn text(&self, language: &str) -> String {
        let da = language == "da";
        let word = self.wake_word.trim();
        let word = if word.is_empty() { "Claude" } else { word };
        let mut parts: Vec<String> = Vec::new();
        if self.group {
            parts.push(if da {
                "Du er med i et Discord-opkald med flere personer. Du hører alle i opkaldet, og de hører dig.".into()
            } else {
                "You are in a Discord call with several people. You hear everyone in the call, and they hear you.".into()
            });
        }
        if self.only_when_called {
            parts.push(if da {
                format!("Svar kun, når nogen siger »{word}«. Hvis ingen siger »{word}«, så sig ingenting og vent.")
            } else {
                format!("Only answer when someone says \"{word}\". If nobody says \"{word}\", say nothing and wait.")
            });
        }
        if self.short {
            parts.push(if da {
                "Hold dine svar korte: en til tre sætninger.".into()
            } else {
                "Keep your answers short: one to three sentences.".into()
            });
        }
        if self.danish {
            parts.push(if da {
                "Tal dansk.".into()
            } else {
                "Speak Danish.".into()
            });
        }
        let own = self.own.trim();
        if !own.is_empty() {
            parts.push(own.chars().take(2000).collect());
        }
        if parts.is_empty() {
            return String::new();
        }
        if da {
            parts.push("Du behøver ikke svare på denne besked – bare følg den.".into());
        } else {
            parts.push("You don't need to reply to this message – just follow it.".into());
        }
        parts.join(" ")
    }
}

/// What an added mixer source records.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MixerSourceKind {
    /// A microphone or other input device (endpoint id).
    #[serde(rename_all = "camelCase")]
    Input { device_id: String },
    /// One program's sound (process loopback by executable name).
    Program { exe: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixerSource {
    pub id: u32,
    pub label: String,
    #[serde(flatten)]
    pub kind: MixerSourceKind,
}

/// An added output: a playback device the user listens to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixerOutput {
    pub id: u32,
    pub label: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixerRoute {
    pub source: u32,
    pub output: u32,
    pub gain_db: f32,
}

/// The advanced mixer. `routes: None` means "the standard routing".
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Mixer {
    pub sources: Vec<MixerSource>,
    pub outputs: Vec<MixerOutput>,
    pub routes: Option<Vec<MixerRoute>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub mixer: Mixer,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub version: u32,
    pub language: String,
    pub theme: Theme,
    pub autostart: bool,
    pub onboarding_done: bool,
    /// The big switch: Claude in the Discord call.
    pub claude_in_discord: bool,
    pub mic_muted: bool,
    pub claude_to_discord_muted: bool,
    pub push_to_talk: bool,
    pub echo: EchoMode,
    pub hotkeys: Hotkeys,
    pub advanced_unlocked: bool,
    /// Keep Discord's microphone pointed at Claude Talk (off after
    /// "Restore normal sound settings" until Claude is switched on again).
    pub route_discord: bool,
    /// Advanced mixer: added sources/outputs and the routing.
    pub mixer: Mixer,
    pub presets: Vec<Preset>,
    /// "Use Claude in Chrome or the app" (only with an extra virtual cable):
    /// the program whose sound is Claude's voice (claude.exe, chrome.exe,
    /// msedge.exe), or none.
    pub external_claude: Option<String>,
    pub instructions: Instructions,
    /// Claude hears the others in the call (off: only you). Enforced by the
    /// routing, not by asking Claude.
    pub claude_hears_discord: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: CONFIG_VERSION,
            language: "da".into(),
            theme: Theme::System,
            autostart: true,
            onboarding_done: false,
            claude_in_discord: true,
            mic_muted: false,
            claude_to_discord_muted: false,
            push_to_talk: false,
            echo: EchoMode::Auto,
            hotkeys: Hotkeys::default(),
            advanced_unlocked: false,
            route_discord: true,
            mixer: Mixer::default(),
            presets: Vec::new(),
            external_claude: None,
            instructions: Instructions::default(),
            claude_hears_discord: true,
        }
    }
}

impl Config {
    /// Loads the file; a missing or damaged file gives the defaults (the
    /// damaged file is kept as `config.json.bad` for the error report).
    pub fn load(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Config::default();
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(c) => c.migrate(),
            Err(e) => {
                log::warn!("config.json unreadable ({e}); using defaults");
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                Config::default()
            }
        }
    }

    fn migrate(mut self) -> Config {
        // Version 1 is the first format; future versions convert here.
        if self.version > CONFIG_VERSION {
            // keep the newer file: going back to that version restores it
            let path = crate::paths::config_file();
            let _ = std::fs::copy(&path, path.with_extension("json.newer"));
            log::warn!(
                "config.json is from a newer Claude Talk (v{}); unknown settings are ignored",
                self.version
            );
        }
        if !["da", "en"].contains(&self.language.as_str()) {
            self.language = "da".into();
        }
        self.version = CONFIG_VERSION;
        self
    }

    /// Writes atomically (temp file + rename) so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_text() {
        let mut i = Instructions {
            group: false,
            ..Default::default()
        };
        assert_eq!(i.text("da"), "");
        i.only_when_called = true;
        i.wake_word = "Robot".into();
        i.short = true;
        let t = i.text("da");
        assert!(t.contains("»Robot«") && t.contains("korte"), "{t}");
        i.wake_word = "  ".into();
        assert!(i.text("en").contains("\"Claude\""));
    }

    #[test]
    fn mixer_settings_roundtrip() {
        let c = Config {
            mixer: Mixer {
                sources: vec![
                    MixerSource {
                        id: 200,
                        label: "Spotify".into(),
                        kind: MixerSourceKind::Program {
                            exe: "spotify.exe".into(),
                        },
                    },
                    MixerSource {
                        id: 201,
                        label: "USB".into(),
                        kind: MixerSourceKind::Input {
                            device_id: "{0.0.1}.{x}".into(),
                        },
                    },
                ],
                outputs: vec![MixerOutput {
                    id: 300,
                    label: "Headset".into(),
                    device_id: "{0.0.0}.{y}".into(),
                }],
                routes: Some(vec![MixerRoute {
                    source: 200,
                    output: 102,
                    gain_db: -6.0,
                }]),
            },
            presets: vec![Preset {
                name: "Musik".into(),
                mixer: Mixer::default(),
            }],
            ..Default::default()
        };
        let text = serde_json::to_string(&c).unwrap();
        assert!(
            text.contains(r#""type":"program","exe":"spotify.exe""#),
            "{text}"
        );
        assert!(text.contains(r#""type":"input","deviceId""#), "{text}");
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back, c);
        // files from before the mixer existed still load
        let old: Config = serde_json::from_str(r#"{"version":1,"language":"en"}"#).unwrap();
        assert_eq!(old.mixer, Mixer::default());
    }
    #[test]
    fn roundtrip_and_damaged_file() {
        let dir = std::env::temp_dir().join(format!("ct-cfg-{}", std::process::id()));
        let path = dir.join("config.json");
        let c = Config {
            language: "en".into(),
            ..Default::default()
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
        assert!(dir.join("config.json.bad").exists());
        // unknown fields and missing fields are tolerated
        std::fs::write(&path, r#"{"version":1,"language":"xx","future":true}"#).unwrap();
        let l = Config::load(&path);
        assert_eq!(l.language, "da");
        assert!(l.autostart);
        let _ = std::fs::remove_dir_all(dir);
    }
}
