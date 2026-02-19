use std::{fs, thread};

/// Read the 1-minute load average from `/proc/loadavg` and normalize it by
/// the number of logical CPU cores, producing a value in `0.0..=1.0`.
///
/// Values above 1.0 (load higher than number of cores) are clamped to 1.0.
/// Returns `None` if `/proc/loadavg` cannot be read or parsed.
pub fn read_cpu_load() -> Option<f64> {
    let content = fs::read_to_string("/proc/loadavg").ok()?;

    // format: "0.52 0.58 0.59 1/512 12345"
    let one_min: f64 = content.split_whitespace().next()?.parse().ok()?;

    let cores = thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0);

    Some((one_min / cores).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_load_is_normalized() {
        // /proc/loadavg is always available on Linux; if this test runs on a
        // Linux host, the result must be in [0.0, 1.0].
        if let Some(load) = read_cpu_load() {
            assert!(
                (0.0..=1.0).contains(&load),
                "cpu load {load} out of [0.0, 1.0]"
            );
        }
    }
}
