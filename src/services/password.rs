//! Secure password generation.
//!
//! Uses `rand::rng()`, a ChaCha-based CSPRNG seeded from the operating system,
//! with uniform index selection (no modulo bias).

use std::collections::HashSet;

use rand::Rng;

use crate::error::ApiError;

pub const MAX_LENGTH: usize = 256;
pub const MAX_QUANTITY: usize = 100;
const MAX_CHARSET: usize = 256;

/// Default alphabet: letters, digits and common symbols.
pub const DEFAULT_CHARSET: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{}";

/// Generates `quantity` passwords of `length` characters.
///
/// `custom_chars` replaces the default alphabet; duplicates are removed so
/// repeated characters cannot skew the distribution.
pub fn generate(
    length: usize,
    quantity: usize,
    custom_chars: Option<&str>,
) -> Result<Vec<String>, ApiError> {
    if !(1..=MAX_LENGTH).contains(&length) {
        return Err(ApiError::BadRequest(format!("length must be between 1 and {MAX_LENGTH}")));
    }
    if !(1..=MAX_QUANTITY).contains(&quantity) {
        return Err(ApiError::BadRequest(format!("quantity must be between 1 and {MAX_QUANTITY}")));
    }

    let alphabet = build_alphabet(custom_chars)?;
    let mut rng = rand::rng();
    Ok((0..quantity)
        .map(|_| {
            (0..length)
                .map(|_| alphabet[rng.random_range(0..alphabet.len())])
                .collect()
        })
        .collect())
}

/// Validates and de-duplicates the character set, preserving order.
fn build_alphabet(custom: Option<&str>) -> Result<Vec<char>, ApiError> {
    let source = custom.unwrap_or(DEFAULT_CHARSET);
    if source.is_empty() {
        return Err(ApiError::BadRequest("chars must not be empty".into()));
    }
    if source.chars().any(char::is_control) {
        return Err(ApiError::BadRequest("chars must not contain control characters".into()));
    }
    let mut seen = HashSet::new();
    let alphabet: Vec<char> = source.chars().filter(|c| seen.insert(*c)).collect();
    if alphabet.len() > MAX_CHARSET {
        return Err(ApiError::BadRequest(format!("chars may contain at most {MAX_CHARSET} unique characters")));
    }
    Ok(alphabet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_length_quantity_and_charset() {
        let pws = generate(16, 5, Some("ab1")).unwrap();
        assert_eq!(pws.len(), 5);
        assert!(pws.iter().all(|p| p.chars().count() == 16 && p.chars().all(|c| "ab1".contains(c))));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(generate(0, 1, None).is_err());
        assert!(generate(8, 0, None).is_err());
        assert!(generate(8, 1, Some("")).is_err());
        assert!(generate(8, 1, Some("a\nb")).is_err());
    }
}
