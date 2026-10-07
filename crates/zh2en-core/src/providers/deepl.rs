use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{api_error, Translator};
use crate::{config::resolve_key, Error, GlossaryEntry, ProviderConfig, Result};

pub struct DeepL {
    client: reqwest::Client,
    url: String,
    api_key: String,
    target_lang: String,
}

impl DeepL {
    pub fn from_config(cfg: &ProviderConfig, client: reqwest::Client) -> Result<Self> {
        let ProviderConfig::Deepl { api_key, api_key_env, target_lang } = cfg else {
            unreachable!("DeepL::from_config called with {cfg:?}")
        };
        let api_key = resolve_key(api_key, api_key_env, "DEEPL_API_KEY")?;
        // Free-plan keys end in ":fx" and use a different host.
        let host = if api_key.ends_with(":fx") { "api-free.deepl.com" } else { "api.deepl.com" };
        Ok(Self {
            client,
            url: format!("https://{host}/v2/translate"),
            api_key,
            target_lang: target_lang.clone(),
        })
    }
}

#[derive(Deserialize)]
struct Resp {
    translations: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    text: String,
}

#[async_trait]
impl Translator for DeepL {
    fn name(&self) -> &str {
        "DeepL"
    }

    async fn translate(&self, text: &str, _glossary: &[GlossaryEntry]) -> Result<String> {
        let resp = self
            .client
            .post(&self.url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .json(&json!({ "text": [text], "source_lang": "ZH", "target_lang": self.target_lang }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        let r: Resp = resp.json().await.map_err(|e| Error::BadResponse(e.to_string()))?;
        r.translations
            .into_iter()
            .next()
            .map(|t| t.text)
            .ok_or_else(|| Error::BadResponse("no translations".into()))
    }
}
