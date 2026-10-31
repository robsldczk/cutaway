//! Word normalization shared by the script parser, the aligner and the comp tools.
//! Ported from Stage 0 (`norm_word`, `norm_words`).

/// Normalizes one word: lower case, curly apostrophes, letters and digits only,
/// apostrophes dropped, repeated letters collapsed ("Whoooaaah" -> "whoah").
pub fn norm_word(w: &str) -> String {
    let mut s = String::with_capacity(w.len());
    for c in w.chars() {
        let c = if c == '\u{2019}' || c == '\u{2018}' {
            '\''
        } else {
            c
        };
        for lc in c.to_lowercase() {
            if lc.is_ascii_lowercase() || lc.is_ascii_digit() {
                s.push(lc);
            } else if let Some(b) = fold_accent(lc) {
                s.push(b);
            }
        }
    }
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if prev != Some(c) {
            out.push(c);
        }
        prev = Some(c);
    }
    out
}

fn fold_accent(c: char) -> Option<char> {
    Some(match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ß' => 's',
        'ç' => 'c',
        'ñ' => 'n',
        _ => return None,
    })
}

fn is_break_char(c: char) -> bool {
    matches!(c, '-' | '\u{2013}' | '\u{2014}' | '\u{2026}')
}

/// Splits text into display tokens: emphasis markers removed, dashes and ellipses
/// treated as spaces (so "time-slop" gives two tokens), "..." too.
pub fn display_tokens(text: &str) -> Vec<String> {
    let cleaned: String = text
        .replace("...", " ")
        .chars()
        .filter(|c| *c != '_' && *c != '*')
        .map(|c| if is_break_char(c) { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|t| !norm_word(t).is_empty())
        .map(str::to_string)
        .collect()
}

/// Normalized words of a text (only tokens that keep at least one letter or digit).
pub fn norm_words(text: &str) -> Vec<String> {
    display_tokens(text).iter().map(|t| norm_word(t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_stage0() {
        assert_eq!(norm_word("Whoooaaah!"), "whoah");
        assert_eq!(norm_word("I\u{2019}m"), "im");
        assert_eq!(norm_word("SCIENCE!"), "science");
        assert_eq!(norm_word("--"), "");
    }

    #[test]
    fn words_split_on_dashes_and_ellipses() {
        assert_eq!(
            norm_words("Mh-hm\u{2026}mh-hm."),
            vec!["mh", "hm", "mh", "hm"]
        );
        assert_eq!(
            norm_words("The first time burrito is _EDIBLE_!"),
            vec!["the", "first", "time", "burito", "is", "edible"]
        );
        assert_eq!(norm_words("just...two"), vec!["just", "two"]);
    }
}
