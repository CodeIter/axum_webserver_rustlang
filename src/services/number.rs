//! Random integer generation.

use rand::Rng;

use crate::error::ApiError;

/// Maximum numbers per request.
pub const MAX_QUANTITY: usize = 1_000;

/// Generates `quantity` integers uniformly distributed in `min..=max`.
pub fn generate(min: i64, max: i64, quantity: usize) -> Result<Vec<i64>, ApiError> {
    if min > max {
        return Err(ApiError::BadRequest(
            "min must be less than or equal to max".into(),
        ));
    }
    if !(1..=MAX_QUANTITY).contains(&quantity) {
        return Err(ApiError::BadRequest(format!(
            "quantity must be between 1 and {MAX_QUANTITY}"
        )));
    }
    let mut rng = rand::rng();
    Ok((0..quantity).map(|_| rng.random_range(min..=max)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_stay_in_bounds() {
        let nums = generate(-5, 5, 500).unwrap();
        assert_eq!(nums.len(), 500);
        assert!(nums.iter().all(|n| (-5..=5).contains(n)));
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(generate(10, 1, 1).is_err());
        assert!(generate(1, 10, 0).is_err());
        assert!(generate(1, 10, MAX_QUANTITY + 1).is_err());
    }

    #[test]
    fn handles_full_i64_range() {
        assert!(generate(i64::MIN, i64::MAX, 3).is_ok());
    }
}
