#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod platform;

pub use platform::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Flaky on CI due to VMs, so run it on a local machine.
    fn test_sleep_accuracy() {
        const THRESHOLD: u64 = 20_000;
        const PERCENTILE: f64 = 99.0;
        const DURATION: u64 = 100_000;
        const ITER: u64 = 1000;

        let below_thresh = (0..ITER)
            .filter(|_| {
                let begin = now();
                sleep_until(begin + DURATION);
                let end = now();
                assert!(end - begin >= DURATION);
                end - begin <= DURATION + THRESHOLD
            })
            .count();
        assert!((below_thresh as f64) >= (ITER as f64) * PERCENTILE / 100.0);
    }
}
