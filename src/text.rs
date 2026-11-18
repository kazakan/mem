//! Language handling: word tokens (Korean josa stripping, stopwords), title shapes, subjects, module tags.

use crate::prelude::*;

pub(crate) const JOSA: &[&str] = &[
    "으로부터",
    "에서부터",
    "이었나요",
    "되었나요",
    "되었는가",
    "되었으며",
    "하였나요",
    "하였는가",
    "하였으며",
    "하였는지",
    "으로서",
    "으로써",
    "에게서",
    "이라고",
    "이라는",
    "이라면",
    "에서는",
    "에서도",
    "에서의",
    "으로는",
    "으로도",
    "에게는",
    "까지는",
    "부터는",
    "였나요",
    "인가요",
    "이었던",
    "되었나",
    "되었고",
    "되었다",
    "되나요",
    "하였고",
    "하였다",
    "했나요",
    "했는가",
    "적으로",
    "인가",
    "이었",
    "였던",
    "되어",
    "되는",
    "했던",
    "했다",
    "하는",
    "하여",
    "해서",
    "하고",
    "하면",
    "하려",
    "한다",
    "이라",
    "에서",
    "에게",
    "한테",
    "으로",
    "께서",
    "까지",
    "부터",
    "마다",
    "조차",
    "처럼",
    "보다",
    "이나",
    "이며",
    "이고",
    "에는",
    "에도",
    "와의",
    "과의",
    "라는",
    "라고",
    "적인",
    "들의",
    "들이",
    "들은",
    "들을",
    "된",
    "될",
    "할",
    "한",
    "은",
    "는",
    "이",
    "가",
    "을",
    "를",
    "의",
    "에",
    "와",
    "과",
    "도",
    "로",
    "만",
    "요",
];

pub(crate) const KO_STOP_L: &[&str] = &[
    "각각",
    "것",
    "경우",
    "관련",
    "그",
    "그러한",
    "그리고",
    "내용",
    "누구",
    "누구인가",
    "누구인가요",
    "대한",
    "대해",
    "되나요",
    "등",
    "따라",
    "따르면",
    "따른",
    "또한",
    "몇",
    "무슨",
    "무엇",
    "무엇이고",
    "무엇이며",
    "무엇인가",
    "무엇인가요",
    "무엇인지",
    "및",
    "서술해주세요",
    "설명",
    "설명하세요",
    "설명하시오",
    "설명해",
    "설명해주세요",
    "수",
    "어느",
    "어디",
    "어디로",
    "어떠",
    "어떠한",
    "어떤",
    "어떻게",
    "언제",
    "언제인가",
    "얼마",
    "얼마나",
    "위한",
    "위해",
    "이",
    "이러한",
    "이를",
    "인가요",
    "있나요",
    "있는",
    "있는지",
    "있으며",
    "저",
    "중",
    "통해",
    "해당",
];

pub(crate) const EN_STOP_L: &[&str] = &[
    "a", "about", "after", "also", "an", "and", "any", "are", "as", "at", "be", "been", "before", "being", "by", "can",
    "could", "did", "do", "does", "done", "down", "during", "for", "from", "had", "has", "have", "he", "her", "here",
    "his", "how", "i", "if", "in", "into", "is", "it", "its", "just", "many", "may", "me", "might", "more", "most",
    "much", "must", "my", "no", "not", "of", "on", "only", "or", "other", "our", "out", "over", "own", "same", "shall",
    "she", "should", "so", "some", "such", "than", "that", "the", "their", "them", "then", "there", "these", "they",
    "this", "those", "to", "too", "up", "very", "was", "we", "were", "what", "when", "where", "which", "while", "who",
    "whom", "whose", "why", "will", "with", "would", "yes", "you", "your",
];

pub(crate) const GENERIC_L: &[&str] =
    &["최신", "변경", "기준", "규칙", "latest", "new", "update", "updated", "changed", "rule", "rules", "정책", "결정"];

pub(crate) const SUBJ_KINDS: &[&str] = &["decision", "convention", "gotcha", "fact"];

pub(crate) static EN_STOP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| EN_STOP_L.iter().copied().collect());

pub(crate) static KO_STOP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| KO_STOP_L.iter().copied().collect());

pub(crate) static GENERIC: LazyLock<HashSet<&'static str>> = LazyLock::new(|| GENERIC_L.iter().copied().collect());

pub(crate) static TOK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9A-Za-z가-힣][0-9A-Za-z가-힣\-\.'·_]*").unwrap());

pub(crate) static SHAPE_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^0-9a-z가-힣]+").unwrap());

pub(crate) static VALUEISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9=:/#%@]").unwrap());

pub(crate) static VALUE_CHUNK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[=:/#%@]|^v?\d+([.,]\d+)?[a-z가-힣]{0,3}$|^[A-Z][A-Z0-9]*_[A-Z0-9_]+$").unwrap());

pub(crate) static UPPER_VAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z_]{2,}$").unwrap());

pub(crate) static SHORT_EN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]{1,3}$").unwrap());

pub(crate) static PLURAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z]{4,}s$").unwrap());

pub(crate) static KW_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[,\n]").unwrap());

pub(crate) fn has_hangul(s: &str) -> bool {
    s.chars().any(|c| ('가'..='힣').contains(&c))
}

pub(crate) fn strip_ko(t: &str) -> &str {
    if has_hangul(t) {
        for s in JOSA {
            if t.ends_with(s) && clen(t) > clen(s) {
                return &t[..t.len() - s.len()];
            }
        }
    }
    t
}

pub(crate) fn tokens(text: &str) -> Vec<String> {
    let mut out: IndexSet<String> = IndexSet::new();
    for m in TOK.find_iter(text) {
        let t = strip_set(m.as_str(), ".-'·_");
        if t.is_empty() || EN_STOP.contains(t.to_lowercase().as_str()) || KO_STOP.contains(t) {
            continue;
        }
        let s = strip_ko(t);
        if !s.is_empty() && !KO_STOP.contains(s) && clen(s) >= 2 {
            out.insert(s.to_string());
        }
    }
    out.into_iter().collect()
}

pub(crate) fn dedup(v: Vec<String>) -> Vec<String> {
    v.into_iter().collect::<IndexSet<_>>().into_iter().collect()
}

pub(crate) fn lower_set(v: &[String]) -> HashSet<String> {
    v.iter().map(|x| x.to_lowercase()).collect()
}

pub(crate) fn qe(t: &str) -> String {
    let q = format!("\"{}\"", t.replace('"', "\"\""));
    if SHORT_EN.is_match(t) {
        q
    } else {
        q + "*"
    }
}

pub(crate) fn shape(t: &str) -> HashSet<String> {
    let low = t.to_lowercase();
    let mut out = HashSet::new();
    for w in SHAPE_SPLIT.split(&low) {
        let w = strip_ko(w);
        if clen(w) < 2 || EN_STOP.contains(w) || KO_STOP.contains(w) {
            continue;
        }
        out.insert(if w.bytes().any(|b| b.is_ascii_digit()) { "<n>".to_string() } else { w.to_string() });
    }
    out
}

pub(crate) fn same_shape(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let c = a.intersection(b).count();
    let j = c as f64 / a.union(b).count() as f64;
    j >= 0.5 || (c >= 3 && j >= 0.4)
}

pub(crate) fn subject(title: &str) -> HashSet<String> {
    tokens(title)
        .into_iter()
        .filter(|t| !VALUEISH.is_match(t) && !UPPER_VAR.is_match(t))
        .map(|t| t.to_lowercase())
        .filter(|t| !GENERIC.contains(t.as_str()))
        .collect()
}

pub(crate) fn subj2(title: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for c in py_split_ws(title) {
        let c = strip_set(c, "()[],.'\"·");
        if c.is_empty() || VALUE_CHUNK.is_match(c) {
            continue;
        }
        for m in TOK.find_iter(c) {
            let mut w = strip_set(m.as_str(), ".-'·_").to_lowercase();
            if has_hangul(&w) {
                let s2 = strip_ko(&w);
                if clen(s2) >= 2 {
                    w = s2.to_string();
                }
            }
            if clen(&w) < 2
                || EN_STOP.contains(w.as_str())
                || KO_STOP.contains(w.as_str())
                || GENERIC.contains(w.as_str())
            {
                continue;
            }
            if PLURAL.is_match(&w) {
                w.pop();
            }
            out.insert(w);
        }
    }
    out
}

pub(crate) fn same_subj(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    a.len() >= 2 && a == b
}

pub(crate) fn sorted_vec(s: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = s.iter().cloned().collect();
    v.sort();
    v
}

pub(crate) fn tags_of(t: &str) -> Vec<String> {
    t.split(',').map(|x| py_strip(x).to_lowercase()).filter(|x| !x.is_empty()).collect()
}

pub(crate) fn module_of(title: &str, tags: &str) -> Option<String> {
    let is_w = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '-';
    let words: HashSet<String> =
        title.split(|c: char| !is_w(c)).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect();
    tags_of(tags).into_iter().find(|t| words.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn josa_is_stripped_from_korean_words() {
        assert_eq!(strip_ko("결정은"), "결정");
        assert_eq!(strip_ko("배포에서부터"), "배포");
        assert_eq!(strip_ko("API는"), "API");
        assert_eq!(strip_ko("은"), "은"); // never strips a word down to nothing
        assert_eq!(strip_ko("deploy"), "deploy");
    }

    #[test]
    fn tokens_drop_stopwords_and_duplicates() {
        assert_eq!(tokens("What is the deploy rule for the wishlist?"), vec!["deploy", "rule", "wishlist"]);
        assert_eq!(tokens("찜 알림은 어디로 가나요? 알림"), vec!["알림", "가나"]);
        assert_eq!(tokens("on-call rotation"), vec!["on-call", "rotation"]);
        assert_eq!(tokens("API는 v2"), vec!["API", "v2"]);
    }

    /// Known issue (docs/DECISIONS.md, D-14): one-letter words and two-syllable words whose last syllable looks
    /// like a josa are dropped. Kept as-is (the output snapshot depends on it) until a fix is chosen.
    #[test]
    fn known_issue_short_words_are_dropped() {
        assert!(tokens("찜").is_empty());
        assert!(tokens("평가").is_empty());
        assert!(tokens("C").is_empty());
        assert!(tokens("IT").is_empty()); // "it" is an English stopword
    }

    #[test]
    fn short_english_words_match_exactly_others_by_prefix() {
        assert_eq!(qe("API"), "\"API\"");
        assert_eq!(qe("deploy"), "\"deploy\"*");
        assert_eq!(qe("알림"), "\"알림\"*");
        assert_eq!(qe("a\"b"), "\"a\"\"b\"*"); // quotes are doubled; not letters-only, so prefix
    }

    #[test]
    fn shape_ignores_numbers_and_josa() {
        assert_eq!(shape("Catalog API owner: team 30"), set(&["catalog", "api", "owner", "team", "<n>"]));
        assert_eq!(shape("team 3"), set(&["team"])); // one-character pieces are dropped before the number check
        assert!(same_shape(&shape("Cart worker count"), &shape("Inventory worker count")));
        assert!(!same_shape(&shape("Cart worker count"), &shape("Refund log format")));
        assert!(!same_shape(&HashSet::new(), &shape("x y")));
    }

    #[test]
    fn subj2_keeps_the_subject_not_the_value() {
        assert_eq!(subj2("Wishlist incident alert channel"), set(&["wishlist", "incident", "alert", "channel"]));
        assert_eq!(subj2("Tax local test env USE_SQLITE=1"), set(&["tax", "local", "test", "env"]));
        assert_eq!(subj2("Tax local test env: sqlite"), set(&["tax", "local", "test", "sqlite"])); // a chunk with ':' is a value
        assert_eq!(subj2("Review channels v2"), set(&["review", "channel"]));
        assert!(same_subj(&subj2("Coupons batch chunk size 500"), &subj2("Coupons batch chunk size 1000")));
        assert!(!same_subj(&subj2("Deploy"), &subj2("Deploy"))); // one word is not enough
    }

    #[test]
    fn subject_drops_values_variables_and_generic_words() {
        assert_eq!(subject("Latest REDIS_URL rule for tax 2026"), set(&["tax"]));
    }

    #[test]
    fn module_is_a_tag_that_also_appears_in_the_title() {
        assert_eq!(module_of("Wishlist SEV2 threshold", "alert, Wishlist"), Some("wishlist".to_string()));
        assert_eq!(module_of("SEV2 threshold", "alert,wishlist"), None);
        assert_eq!(tags_of(" A, ,b "), vec!["a", "b"]);
    }
}
