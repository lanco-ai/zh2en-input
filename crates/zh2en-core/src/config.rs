//! User configuration, stored as JSON (one file, hand-editable).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Global hotkey that opens / closes the popup, in Tauri accelerator
    /// syntax, e.g. "CommandOrControl+Shift+Space", "Alt+Shift+E".
    pub hotkey: String,
    pub provider: ProviderConfig,
    /// Terms that must always be translated a fixed way (names, jargon).
    pub glossary: Vec<GlossaryEntry>,
    /// After Enter: paste into the previous window (true) or only copy (false).
    pub auto_paste: bool,
    /// How text reaches the target app: "paste" (clipboard + Ctrl/Cmd+V, fast,
    /// works almost everywhere) or "type" (simulated keystrokes, for apps
    /// that block pasting).
    pub commit_method: CommitMethod,
    /// Put the user's previous clipboard text back after pasting.
    pub restore_clipboard: bool,
    /// Wait between refocusing the target window and sending the paste keys.
    pub paste_delay_ms: u64,
    /// Idle time after the last keystroke before a preview is requested.
    pub preview_debounce_ms: u64,
    /// Per-request timeout for the translation provider.
    pub timeout_ms: u64,
    /// Number of translations kept in the in-memory cache.
    pub cache_size: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommitMethod {
    Paste,
    Type,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlossaryEntry {
    pub zh: String,
    pub en: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProviderConfig {
    /// Any OpenAI-compatible chat-completions endpoint: DeepSeek, Qwen
    /// (DashScope compatible mode), Moonshot, OpenAI, a local Ollama, …
    Openai {
        base_url: String,
        model: String,
        #[serde(default)]
        api_key: Option<String>,
        /// Environment variable to read the key from when `api_key` is empty.
        #[serde(default)]
        api_key_env: Option<String>,
        #[serde(default)]
        temperature: Option<f32>,
        /// Optional extra instruction, e.g. "casual chat tone" or
        /// "formal business email".
        #[serde(default)]
        style: Option<String>,
    },
    /// DeepL REST API. The glossary is not applied for DeepL in this MVP.
    Deepl {
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default)]
        api_key_env: Option<String>,
        /// "EN-US" or "EN-GB".
        #[serde(default = "default_deepl_target")]
        target_lang: String,
    },
    /// Offline stand-in that returns "[EN] <input>"; for UI development.
    Mock,
}

fn default_deepl_target() -> String {
    "EN-US".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".into(),
            provider: ProviderConfig::Openai {
                base_url: "https://api.deepseek.com/v1".into(),
                model: "deepseek-chat".into(),
                api_key: None,
                api_key_env: Some("DEEPSEEK_API_KEY".into()),
                temperature: Some(0.2),
                style: None,
            },
            glossary: vec![],
            auto_paste: true,
            commit_method: CommitMethod::Paste,
            restore_clipboard: true,
            paste_delay_ms: 120,
            preview_debounce_ms: 350,
            timeout_ms: 8000,
            cache_size: 512,
        }
    }
}

impl Config {
    /// Read the config at `path`, creating it with defaults if it is missing.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| Error::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Self::default();
                cfg.save(path)?;
                Ok(cfg)
            }
            Err(e) => Err(Error::Config(format!("{}: {e}", path.display()))),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Config(e.to_string()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(path, json + "\n").map_err(|e| Error::Config(e.to_string()))
    }
}

/// Resolve an API key from the inline value or the named environment variable.
pub(crate) fn resolve_key(inline: &Option<String>, env: &Option<String>, fallback_env: &str) -> Result<String> {
    if let Some(k) = inline.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        return Ok(k.to_string());
    }
    let var = env.as_deref().unwrap_or(fallback_env);
    std::env::var(var)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| Error::MissingApiKey(var.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_default_then_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.json");
        let created = Config::load_or_create(&path).unwrap();
        assert_eq!(created, Config::default());
        assert!(path.exists());
        assert_eq!(Config::load_or_create(&path).unwrap(), created);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{ "provider": { "kind": "mock" }, "glossary": [{"zh":"飞书","en":"Lark"}] }"#,
        )
        .unwrap();
        let cfg = Config::load_or_create(&path).unwrap();
        assert_eq!(cfg.provider, ProviderConfig::Mock);
        assert_eq!(cfg.glossary.len(), 1);
        assert_eq!(cfg.hotkey, Config::default().hotkey);
    }

    #[test]
    fn bad_json_is_a_config_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(Config::load_or_create(&path), Err(Error::Config(_))));
    }

    #[test]
    fn inline_key_wins_over_env() {
        let k = resolve_key(&Some(" sk-1 ".into()), &Some("ZH2EN_TEST_UNSET_VAR".into()), "X").unwrap();
        assert_eq!(k, "sk-1");
        assert!(matches!(
            resolve_key(&None, &Some("ZH2EN_TEST_UNSET_VAR".into()), "X"),
            Err(Error::MissingApiKey(v)) if v == "ZH2EN_TEST_UNSET_VAR"
        ));
    }
}
