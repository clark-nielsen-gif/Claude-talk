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
            mute_claude_to_discord: "Ctrl+Alt+D".into(),
            mute_mic: "Ctrl+Alt+M".into(),
            push_to_talk_claude: "Ctrl+Alt+Space".into(),
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
        }
    }
}

impl Config {
    /// Loads the file; a missing or damaged file gives the defaults (the
    /// damaged file is kept as `config.json.bad` for the error report).
    pub fn load(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else { return Config::default() };
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
            log::warn!("config.json is from a newer Claude Talk (v{}); unknown settings are ignored", self.version);
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
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_damaged_file() {
        let dir = std::env::temp_dir().join(format!("ct-cfg-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut c = Config::default();
        c.language = "en".into();
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
