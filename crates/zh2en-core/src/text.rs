//! Small text helpers: deciding whether to translate, and cleaning model output.

/// True if `s` contains at least one Han character. Text without any Han
/// characters (already English, a URL, a number) is passed through untouched
/// so the tool never "translates" English into worse English.
pub fn contains_han(s: &str) -> bool {
    s.chars().any(is_han)
}

fn is_han(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF   // CJK Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0x20000..=0x2EBEF // Extensions B–F
        | 0x30000..=0x323AF // Extensions G–H
    )
}

/// Clean a raw model reply into the text that should be committed:
/// - drops `<think>…</think>` blocks emitted by reasoning models
/// - trims whitespace
/// - removes one pair of wrapping quotes the model added on its own
///   (kept if the source itself was quoted)
pub fn clean_output(source: &str, raw: &str) -> String {
    let mut out = strip_think(raw).trim().to_string();

    const PAIRS: [(char, char); 4] = [('"', '"'), ('“', '”'), ('\'', '\''), ('「', '」')];
    let src_quoted = source.trim().starts_with(['"', '“', '「', '\'', '‘']);
    if src_quoted {
        return out;
    }
    for (open, close) in PAIRS {
        if out.chars().count() >= 2
            && out.starts_with(open)
            && out.ends_with(close)
        {
            let inner = &out[open.len_utf8()..out.len() - close.len_utf8()];
            // Only strip when the quotes really wrap the whole reply.
            if !inner.contains(open) && !inner.contains(close) {
                out = inner.trim().to_string();
            }
            break;
        }
    }
    out
}

fn strip_think(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_han() {
        assert!(contains_han("你好"));
        assert!(contains_han("hello 世界"));
        assert!(!contains_han("hello, world!"));
        assert!(!contains_han("。，！"));
        assert!(!contains_han(""));
    }

    #[test]
    fn strips_think_and_quotes() {
        assert_eq!(clean_output("你好", "<think>hmm</think>\n\"Hello\""), "Hello");
        assert_eq!(clean_output("你好", "“Hello”"), "Hello");
        assert_eq!(clean_output("你好", "  Hello there.  "), "Hello there.");
    }

    #[test]
    fn keeps_quotes_when_source_quoted() {
        assert_eq!(clean_output("“你好”", "“Hello”"), "“Hello”");
    }

    #[test]
    fn keeps_inner_quotes() {
        assert_eq!(
            clean_output("他说", "\"Yes\" he said \"no\""),
            "\"Yes\" he said \"no\""
        );
    }

    #[test]
    fn unterminated_think_is_dropped() {
        assert_eq!(clean_output("你好", "Hi<think>partial"), "Hi");
    }
}
