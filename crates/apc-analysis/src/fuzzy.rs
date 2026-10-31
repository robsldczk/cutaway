//! Indel similarity (`rapidfuzz.fuzz.ratio`): `100 · 2·LCS / (len a + len b)`, with a
//! bit-parallel LCS (Hyyrö) so the aligner can score millions of windows quickly.

/// A string prepared for repeated comparisons.
pub struct Pattern {
    len: usize,
    words: usize,
    /// match masks: 256 byte values × `words` u64 blocks
    masks: Vec<u64>,
}

impl Pattern {
    pub fn new(s: &str) -> Self {
        let b = s.as_bytes();
        let words = b.len().div_ceil(64).max(1);
        let mut masks = vec![0u64; 256 * words];
        for (i, &c) in b.iter().enumerate() {
            masks[c as usize * words + i / 64] |= 1u64 << (i % 64);
        }
        Pattern {
            len: b.len(),
            words,
            masks,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Length of the longest common subsequence with `other` (bytes).
    pub fn lcs(&self, other: &[u8]) -> usize {
        if self.len == 0 || other.is_empty() {
            return 0;
        }
        let w = self.words;
        let mut v = vec![!0u64; w];
        for &c in other {
            let base = c as usize * w;
            let mut carry = 0u64;
            for (k, vk) in v.iter_mut().enumerate() {
                let m = self.masks[base + k];
                let u = *vk & m;
                let (s1, c1) = vk.overflowing_add(u);
                let (s2, c2) = s1.overflowing_add(carry);
                carry = (c1 | c2) as u64;
                *vk = s2 | (*vk & !m);
            }
        }
        let mut zeros = 0;
        for (k, vk) in v.iter().enumerate() {
            let bits = if k == w - 1 && !self.len.is_multiple_of(64) {
                self.len % 64
            } else {
                64
            };
            let mask = if bits == 64 {
                !0u64
            } else {
                (1u64 << bits) - 1
            };
            zeros += (!vk & mask).count_ones() as usize;
        }
        zeros
    }

    /// `fuzz.ratio` against `other` (0..100).
    pub fn ratio(&self, other: &str) -> f64 {
        let total = self.len + other.len();
        if total == 0 {
            return 100.0;
        }
        200.0 * self.lcs(other.as_bytes()) as f64 / total as f64
    }

    /// Upper bound of `ratio` from lengths alone.
    pub fn max_ratio(&self, other_len: usize) -> f64 {
        let total = self.len + other_len;
        if total == 0 {
            return 100.0;
        }
        200.0 * self.len.min(other_len) as f64 / total as f64
    }
}

/// One-off ratio.
pub fn ratio(a: &str, b: &str) -> f64 {
    Pattern::new(a).ratio(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcs_dp(a: &[u8], b: &[u8]) -> usize {
        let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for i in 0..a.len() {
            for j in 0..b.len() {
                dp[i + 1][j + 1] = if a[i] == b[j] {
                    dp[i][j] + 1
                } else {
                    dp[i][j + 1].max(dp[i + 1][j])
                };
            }
        }
        dp[a.len()][b.len()]
    }

    #[test]
    fn matches_dynamic_programming_and_rapidfuzz() {
        // rapidfuzz: fuzz.ratio("this is a test", "this is a test!") == 96.55...
        assert!((ratio("this is a test", "this is a test!") - 96.551724).abs() < 1e-4);
        assert_eq!(ratio("lunch", "lunch"), 100.0);
        let a = "i put this in the time spring two weks ago and a bit more text to cross sixty four bytes";
        let b =
            "i put this in the time machine and then some other words to make it long enough ok";
        assert_eq!(
            Pattern::new(a).lcs(b.as_bytes()),
            lcs_dp(a.as_bytes(), b.as_bytes())
        );
        let mut seed = 7u64;
        for _ in 0..200 {
            let mut gen = |max: usize| -> String {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let n = 1 + (seed >> 40) as usize % max;
                (0..n)
                    .map(|_| {
                        seed = seed
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        (b'a' + ((seed >> 33) % 5) as u8) as char
                    })
                    .collect()
            };
            let x = gen(150);
            let y = gen(90);
            assert_eq!(
                Pattern::new(&x).lcs(y.as_bytes()),
                lcs_dp(x.as_bytes(), y.as_bytes())
            );
        }
    }
}
