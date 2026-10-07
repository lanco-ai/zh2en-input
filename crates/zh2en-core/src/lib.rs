//! zh2en-core — the translation core shared by every front end
//! (the hotkey popup today; a Rime plugin or local daemon later).
//!
//! It knows nothing about windows, hotkeys or clipboards. It turns Chinese
//! text into English through a pluggable [`Translator`], with an LRU cache,
//! a user glossary and a per-request timeout.

pub mod config;
mod engine;
pub mod providers;
pub mod text;

pub use config::{Config, GlossaryEntry, ProviderConfig};
pub use engine::{Engine, Translation};
pub use providers::Translator;

/// Errors surfaced to the UI. Messages are written for the end user.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("未配置 API Key：请在配置文件里填写 api_key，或设置环境变量 {0}")]
    MissingApiKey(String),
    #[error("翻译超时（{0} ms）")]
    Timeout(u64),
    #[error("网络错误：{0}")]
    Http(String),
    #[error("翻译服务返回错误 {status}：{body}")]
    Api { status: u16, body: String },
    #[error("翻译服务返回了无法解析的结果：{0}")]
    BadResponse(String),
    #[error("配置文件错误：{0}")]
    Config(String),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
