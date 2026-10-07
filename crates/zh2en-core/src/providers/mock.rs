use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Translator;
use crate::{GlossaryEntry, Result};

/// Offline provider: returns "[EN] <input>" with glossary terms substituted.
/// Counts calls so tests can check the cache.
#[derive(Default)]
pub struct Mock {
    pub calls: AtomicUsize,
}

#[async_trait]
impl Translator for Mock {
    fn name(&self) -> &str {
        "mock"
    }

    async fn translate(&self, text: &str, glossary: &[GlossaryEntry]) -> Result<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut out = text.to_string();
        for g in glossary {
            out = out.replace(&g.zh, &g.en);
        }
        Ok(format!("[EN] {out}"))
    }
}
