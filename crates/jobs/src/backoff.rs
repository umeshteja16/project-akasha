//! Retry delays.

use std::time::Duration;

/// Delay before the first retry.
const BASE: Duration = Duration::from_secs(10);
/// Longest delay between attempts.
const CAP: Duration = Duration::from_secs(60 * 60);

/// Delay after failed attempt number `attempt` (1-based): exponential
/// (10 s, 20 s, 40 s, … capped at 1 h) with "equal jitter": half of it fixed, half
/// scaled by `unit` (a random number in `[0, 1)`), so retries of jobs that failed
/// together spread out.
pub fn backoff(attempt: i32, unit: f64) -> Duration {
    let exponent = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(0)
        .min(20);
    let full = BASE.saturating_mul(1 << exponent).min(CAP);
    let half = full / 2;
    half + half.mul_f64(unit.clamp(0.0, 1.0))
}

/// A random number in `[0, 1)` from a v4 UUID's random bits (no extra RNG crate).
pub(crate) fn random_unit() -> f64 {
    const MANTISSA: u32 = 53;
    let bits = (uuid::Uuid::new_v4().as_u128() as u64) & ((1 << MANTISSA) - 1);
    bits as f64 / (1u64 << MANTISSA) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_exponentially_with_bounded_jitter() {
        assert_eq!(backoff(1, 0.0), Duration::from_secs(5));
        assert_eq!(backoff(1, 1.0), Duration::from_secs(10));
        assert_eq!(backoff(2, 0.0), Duration::from_secs(10));
        assert_eq!(backoff(3, 0.5), Duration::from_secs(30));
        assert_eq!(backoff(50, 1.0), CAP);
        assert_eq!(backoff(0, 1.0), BASE);
        for _ in 0..100 {
            let r = random_unit();
            assert!((0.0..1.0).contains(&r));
        }
    }
}
