use lru::LruCache;
use serde::Serialize;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{providers, text, Config, Error, GlossaryEntry, Result, Translator};

/// A finished translation, as sent to the UI.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Translation {
    pub source: String,
    pub text: String,
    /// True if served from cache or passed through without a network call.
    pub cached: bool,
    pub elapsed_ms: u64,
    pub provider: String,
}

pub struct Engine {
    translator: Arc<dyn Translator>,
    glossary: Vec<GlossaryEntry>,
    timeout: Duration,
    cache: Mutex<LruCache<String, String>>,
}

impl Engine {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        Ok(Self::new(providers::build(&cfg.provider)?, cfg.glossary.clone(), cfg.timeout_ms, cfg.cache_size))
    }

    pub fn new(translator: Arc<dyn Translator>, glossary: Vec<GlossaryEntry>, timeout_ms: u64, cache_size: usize) -> Self {
        Self {
            translator,
            glossary,
            timeout: Duration::from_millis(timeout_ms.max(500)),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(cache_size.max(1)).unwrap())),
        }
    }

    pub fn provider_name(&self) -> &str {
        self.translator.name()
    }

    pub async fn translate(&self, input: &str) -> Result<Translation> {
        let started = Instant::now();
        let source = input.trim().to_string();
        let done = |text: String, cached: bool| Translation {
            source: source.clone(),
            text,
            cached,
            elapsed_ms: started.elapsed().as_millis() as u64,
            provider: self.translator.name().to_string(),
        };

        // Nothing Chinese to translate: commit exactly what was typed.
        if !text::contains_han(&source) {
            return Ok(done(source.clone(), true));
        }
        if let Some(hit) = self.cache.lock().unwrap().get(&source).cloned() {
            return Ok(done(hit, true));
        }

        let raw = tokio::time::timeout(self.timeout, self.translator.translate(&source, &self.glossary))
            .await
            .map_err(|_| Error::Timeout(self.timeout.as_millis() as u64))??;
        let out = text::clean_output(&source, &raw);
        if out.is_empty() {
            return Err(Error::BadResponse("empty translation".into()));
        }
        self.cache.lock().unwrap().put(source.clone(), out.clone());
        Ok(done(out, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Mock;
    use async_trait::async_trait;
    use std::sync::atomic::Ordering;

    fn engine(mock: Arc<Mock>) -> Engine {
        Engine::new(mock, vec![GlossaryEntry { zh: "飞书".into(), en: "Lark".into() }], 2000, 8)
    }

    #[tokio::test]
    async fn caches_repeated_text() {
        let mock = Arc::new(Mock::default());
        let e = engine(mock.clone());
        let a = e.translate("  在飞书上聊 ").await.unwrap();
        assert_eq!(a.text, "[EN] 在Lark上聊");
        assert!(!a.cached);
        let b = e.translate("在飞书上聊").await.unwrap();
        assert!(b.cached);
        assert_eq!(b.text, a.text);
        assert_eq!(mock.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn passes_through_non_chinese() {
        let mock = Arc::new(Mock::default());
        let e = engine(mock.clone());
        let t = e.translate("LGTM 👍").await.unwrap();
        assert_eq!(t.text, "LGTM 👍");
        assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
    }

    struct Slow;
    #[async_trait]
    impl Translator for Slow {
        fn name(&self) -> &str {
            "slow"
        }
        async fn translate(&self, _: &str, _: &[GlossaryEntry]) -> Result<String> {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok("late".into())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn times_out() {
        let e = Engine::new(Arc::new(Slow), vec![], 1000, 8);
        assert!(matches!(e.translate("你好").await, Err(Error::Timeout(1000))));
    }
}
