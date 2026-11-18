//! Python string semantics the output format depends on (code-point slicing, str.split/strip/splitlines, int compare).
//! The Python reference (`reference/mem53.py`) defines the output; these keep Rust byte-identical to it.

use crate::prelude::*;

pub(crate) fn clen(s: &str) -> usize {
    s.chars().count()
}

/// s[:n] in code points (n >= 0)
pub(crate) fn cut(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// end index of seq[:n] for Python slicing with a possibly negative n
pub(crate) fn py_end(len: usize, n: i64) -> usize {
    if n >= 0 {
        (n as usize).min(len)
    } else {
        len.saturating_sub(n.unsigned_abs() as usize)
    }
}

pub(crate) fn cut_py(s: &str, n: i64) -> &str {
    cut(s, py_end(clen(s), n))
}

pub(crate) fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

pub(crate) fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

pub(crate) fn py_split_ws(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_py_space).filter(|x| !x.is_empty())
}

pub(crate) fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let brk =
            matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if brk {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' {
                if let Some(&(j, '\n')) = it.peek() {
                    it.next();
                    end = j + 1;
                }
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

pub(crate) fn strip_set<'a>(s: &'a str, set: &str) -> &'a str {
    s.trim_matches(|c| set.contains(c))
}

/// Python's int(digits) comparison without overflow
pub(crate) fn num_cmp_gt(a: &str, b: &str) -> bool {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    (a.len(), a) > (b.len(), b)
}

pub(crate) fn num_of(i: &str) -> String {
    match TRAIL_NUM.captures(i) {
        Some(c) => {
            let d = c.get(1).unwrap().as_str();
            if d.bytes().all(|x| x.is_ascii_digit()) {
                d.to_string()
            } else {
                d.chars().map(|ch| char::from_digit(ch.to_digit(10).unwrap_or(0), 10).unwrap()).collect()
            }
        }
        None => "0".into(),
    }
}

pub(crate) fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let s = format!("{:?}", f);
    s
}

pub(crate) static TRAIL_NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)$").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slicing_counts_code_points() {
        assert_eq!(clen("찜 알림😀"), 5);
        assert_eq!(cut("찜 알림😀", 3), "찜 알");
        assert_eq!(cut("abc", 10), "abc");
        assert_eq!(cut_py("abcdef", -2), "abcd");
        assert_eq!(cut_py("ab", -5), "");
        assert_eq!(py_end(10, 3), 3);
        assert_eq!(py_end(10, -3), 7);
    }

    #[test]
    fn whitespace_matches_python() {
        // str.split() also splits on \x1c-\x1f and the ideographic space
        assert_eq!(py_split_ws("a\x1cb\u{3000}c  d").collect::<Vec<_>>(), vec!["a", "b", "c", "d"]);
        assert_eq!(py_strip("\u{3000} x \t"), "x");
        assert_eq!(strip_set("--a-b--", "-"), "a-b");
    }

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(py_splitlines("a\r\nb\rc\x0bd\u{2028}e\n"), vec!["a", "b", "c", "d", "e"]);
        assert_eq!(py_splitlines(""), Vec::<&str>::new());
        assert_eq!(py_splitlines("x\n\ny"), vec!["x", "", "y"]);
    }

    #[test]
    fn id_numbers_compare_as_integers() {
        assert_eq!(num_of("sh-deci-012"), "012");
        assert_eq!(num_of("no-digits"), "0");
        assert!(num_cmp_gt("010", "9"));
        assert!(!num_cmp_gt("5", "005"));
        assert!(num_cmp_gt("100000000000000000000000", "99999999999999999999999"));
    }

    #[test]
    fn float_repr_like_python() {
        assert_eq!(py_float_repr(1.0), "1.0");
        assert_eq!(py_float_repr(0.1), "0.1");
        assert_eq!(py_float_repr(f64::NAN), "nan");
        assert_eq!(py_float_repr(f64::NEG_INFINITY), "-inf");
    }
}
