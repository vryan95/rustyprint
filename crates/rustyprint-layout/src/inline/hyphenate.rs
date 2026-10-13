//! Automatic hyphenation with the Knuth–Liang patterns of the `hyphenation` crate.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use hyphenation::{Hyphenator, Language, Load, Standard};

/// The dictionary for a BCP 47 language tag, loaded once and shared.
fn dictionary(lang: &str) -> Option<Arc<Standard>> {
    static CACHE: OnceLock<Mutex<HashMap<Language, Option<Arc<Standard>>>>> = OnceLock::new();
    let language = language(lang)?;
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(language)
        .or_insert_with(|| Standard::from_embedded(language).ok().map(Arc::new))
        .clone()
}

/// The dictionary language for a tag: an exact match, a known default for the primary
/// language, or the primary language itself.
fn language(tag: &str) -> Option<Language> {
    let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
    if let Some(language) = Language::try_from_code(&tag) {
        return Some(language);
    }
    let primary = tag.split('-').next().unwrap_or("");
    let region = tag.split('-').nth(1).unwrap_or("");
    let code = match (primary, region) {
        ("en", "us" | "ca" | "") => "en-us",
        ("en", _) => "en-gb",
        ("de", "ch" | "li") => "de-ch-1901",
        ("de", _) => "de-1996",
        ("el", _) => "el-monoton",
        ("sr", _) => "sr-cyrl",
        ("mn", _) => "mn-cyrl",
        ("sh", _) => "sh-latn",
        ("no", _) => "nb",
        ("zh", _) => "zh-latn-pinyin",
        (primary, _) => primary,
    };
    Language::try_from_code(code)
}

/// Byte offsets inside `word` where it may be hyphenated, honoring the minimum number of
/// characters before and after a hyphen.
pub(crate) fn points(word: &str, lang: &str, before: usize, after: usize) -> Vec<usize> {
    let Some(dictionary) = dictionary(lang) else {
        return Vec::new();
    };
    let chars = word.chars().count();
    dictionary
        .hyphenate(word)
        .breaks
        .into_iter()
        .filter(|&b| word.is_char_boundary(b))
        .filter(|&b| {
            let left = word[..b].chars().count();
            left >= before && chars - left >= after
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyphenates_known_languages() {
        let breaks = points("hyphenation", "en", 2, 2);
        assert!(!breaks.is_empty());
        assert!(breaks.contains(&2) || breaks.contains(&6));
        assert!(!points("automatique", "fr-BE", 2, 2).is_empty());
        assert!(!points("Silbentrennung", "de", 2, 2).is_empty());
        assert!(points("hyphenation", "xx", 2, 2).is_empty());
    }

    #[test]
    fn limits_apply() {
        let all = points("hyphenation", "en-US", 1, 1);
        let limited = points("hyphenation", "en-US", 4, 4);
        assert!(limited.len() < all.len());
        assert!(
            limited
                .iter()
                .all(|&b| b >= 4 && b <= "hyphenation".len() - 4)
        );
    }
}
