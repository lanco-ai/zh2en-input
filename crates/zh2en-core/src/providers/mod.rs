//! Translation back ends. Add a new one by implementing [`Translator`]
//! and wiring it into [`build`].

mod deepl;
mod mock;
mod openai;

pub use deepl::DeepL;
pub use mock::Mock;
pub use openai::OpenAiCompatible;

use async_trait::async_trait;
use std::sync::Arc;

use crate::{config::GlossaryEntry, ProviderConfig, Result};

#[async_trait]
pub trait Translator: Send + Sync {
    /// Short label shown in the popup status line, e.g. "deepseek-chat".
    fn name(&self) -> &str;

    /// Translate one piece of Chinese text into English. Returns the raw
    /// reply; the engine cleans it with [`crate::text::clean_output`].
    async fn translate(&self, text: &str, glossary: &[GlossaryEntry]) -> Result<String>;
}

/// Build the configured provider. Fails fast on a missing API key so the
/// user sees the problem as soon as the popup opens.
pub fn build(cfg: &ProviderConfig) -> Result<Arc<dyn Translator>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("zh2en-input/", env!("CARGO_PKG_VERSION")))
        .build()?;
    Ok(match cfg {
        ProviderConfig::Openai { .. } => Arc::new(OpenAiCompatible::from_config(cfg, client)?),
        ProviderConfig::Deepl { .. } => Arc::new(DeepL::from_config(cfg, client)?),
        ProviderConfig::Mock => Arc::new(Mock::default()),
    })
}

/// Read a JSON error body into a short message for the user.
pub(crate) async fn api_error(resp: reqwest::Response) -> crate::Error {
    let status = resp.status().as_u16();
    let mut body = resp.text().await.unwrap_or_default();
    if body.chars().count() > 300 {
        body = body.chars().take(300).collect::<String>() + "…";
    }
    crate::Error::Api { status, body }
}
