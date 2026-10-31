//! Maps heard words onto script words (LCS with fuzzy word equality), so comps can take
//! the same script word from any take.

use crate::fuzzy::ratio;

fn same(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 4 && b.len() >= 4 && ratio(a, b) >= 80.0)
}

/// For each heard word, the index of the script word it performs.
pub fn map_words(heard: &[String], script: &[String]) -> Vec<Option<usize>> {
    let (n, m) = (heard.len(), script.len());
    let mut dp = vec![0u16; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[at(i, j)] = if same(&heard[i], &script[j]) {
                dp[at(i + 1, j + 1)] + 1
            } else {
                dp[at(i + 1, j)].max(dp[at(i, j + 1)])
            };
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if same(&heard[i], &script[j]) && dp[at(i, j)] == dp[at(i + 1, j + 1)] + 1 {
            out[i] = Some(j);
            i += 1;
            j += 1;
        } else if dp[at(i + 1, j)] >= dp[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Unmatched heard words between two mapped ones take the nearest earlier script word,
    // so every heard word belongs somewhere for comping.
    let mut last: Option<usize> = None;
    for o in out.iter_mut() {
        match o {
            Some(v) => last = Some(*v),
            None => *o = last,
        }
    }
    out
}

/// Share of script words covered by the mapping.
pub fn coverage(map: &[Option<usize>], script_len: usize) -> f64 {
    if script_len == 0 {
        return 0.0;
    }
    let mut seen = vec![false; script_len];
    for v in map.iter().flatten() {
        seen[*v] = true;
    }
    seen.iter().filter(|x| **x).count() as f64 / script_len as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn maps_with_skips_and_compounds() {
        let script = w("i put this in the time spring two weks ago");
        let m = map_words(&w("i put this in the time machine"), &script);
        assert_eq!(
            m,
            vec![
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(5)
            ]
        );
        assert!((coverage(&m, script.len()) - 0.6).abs() < 1e-9);
        let m = map_words(&w("i i put this"), &script);
        assert_eq!(m[2], Some(1));
    }
}
