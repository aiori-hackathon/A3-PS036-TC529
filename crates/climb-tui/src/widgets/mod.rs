//! Reusable widgets for the performance tabs (phases 20.1, 21.1).

pub mod ladder_view;
pub mod log_bars;
pub mod stacked_bars;
pub mod udp_gauge;

/// Truncate `text` to at most `max` columns, ending with an ellipsis marker
/// when characters are dropped.
///
/// The marker is `\u{2026}` (one column) in the normal glyph set and `~` in
/// ASCII mode. Shortening counts characters, so it never splits a character,
/// and `max == 0` yields an empty string. Use this for text drawn by a widget
/// that clips at the panel edge: an explicit marker reads as truncation rather
/// than a word chopped in half.
pub fn ellipsize(text: &str, max: usize, ascii: bool) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let Some(keep) = max.checked_sub(1) else {
        return String::new();
    };
    let head: String = text.chars().take(keep).collect();
    format!("{head}{}", if ascii { '~' } else { '\u{2026}' })
}

/// Ellipsize every whitespace-separated token of `text` that is longer than
/// `max` columns, leaving shorter tokens untouched.
///
/// Use this for text a wrapping paragraph will lay out: the wrapper breaks at
/// spaces, so pre-shortening an over-long token stops the one case where the
/// wrapper would have to split a word across two lines. Whitespace is otherwise
/// preserved so the paragraph keeps its original shape.
pub fn ellipsize_tokens(text: &str, max: usize, ascii: bool) -> String {
    if max == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(text.len());
    for (i, token) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if token.chars().count() > max {
            out.push_str(&ellipsize(token, max, ascii));
        } else {
            out.push_str(token);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsize_keeps_text_that_fits() {
        assert_eq!(ellipsize("Small", 5, false), "Small");
        assert_eq!(ellipsize("Small", 20, false), "Small");
        assert_eq!(ellipsize("", 3, false), "");
    }

    #[test]
    fn ellipsize_appends_a_marker_and_counts_the_marker_in_the_limit() {
        let unicode = ellipsize("Without MTL", 8, false);
        assert_eq!(unicode, "Without\u{2026}");
        assert_eq!(unicode.chars().count(), 8);
        assert_eq!(ellipsize("Without MTL", 8, true), "Without~");
        assert_eq!(ellipsize("abcdef", 1, false), "\u{2026}");
    }

    #[test]
    fn ellipsize_of_zero_columns_is_empty() {
        assert_eq!(ellipsize("anything", 0, false), "");
    }

    #[test]
    fn ellipsize_never_splits_a_multibyte_character() {
        let out = ellipsize("caf\u{00e9} latte", 5, false);
        assert_eq!(out.chars().count(), 5);
        assert!(out.starts_with("caf\u{00e9}"), "{out}");
        assert!(out.ends_with('\u{2026}'), "{out}");
    }

    #[test]
    fn ellipsize_tokens_only_shortens_over_long_tokens() {
        assert_eq!(ellipsize_tokens("a bb cc", 4, false), "a bb cc");
        assert_eq!(ellipsize_tokens("a bbbbb", 4, false), "a bbb\u{2026}");
        assert_eq!(ellipsize_tokens("bbbbbb", 4, true), "bbb~");
    }

    #[test]
    fn ellipsize_tokens_preserves_whitespace_and_empty_input() {
        assert_eq!(ellipsize_tokens("", 4, false), "");
        assert_eq!(ellipsize_tokens("a  bbbbb", 4, false), "a  bbb\u{2026}");
        assert_eq!(ellipsize_tokens("anything", 0, false), "");
    }
}
