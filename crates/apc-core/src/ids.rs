//! Stable short identifiers (`ln_3kq9z0x1ab`). IDs never change once assigned.

const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// A new random ID with a readable prefix, e.g. `new_id("ln")`.
pub fn new_id(prefix: &str) -> String {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let mut n = u128::from_be_bytes(bytes);
    let mut out = String::with_capacity(prefix.len() + 13);
    out.push_str(prefix);
    out.push('_');
    for _ in 0..12 {
        out.push(ALPHABET[(n % 32) as usize] as char);
        n /= 32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_and_unique() {
        let a = new_id("ln");
        let b = new_id("ln");
        assert!(a.starts_with("ln_"));
        assert_eq!(a.len(), 15);
        assert_ne!(a, b);
    }
}
