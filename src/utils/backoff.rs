use std::time::Duration;

/// Exponential backoff for reconnect loops.
///
/// Each call to [`Backoff::next`] returns the current delay and doubles it
/// for the following call, up to `max`. [`Backoff::reset`] starts over from
/// `initial`, typically after a connection proved stable.
///
/// # Example
/// ```ignore
/// let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
/// assert_eq!(backoff.next(), Duration::from_secs(1));
/// assert_eq!(backoff.next(), Duration::from_secs(2));
/// ```
#[derive(Debug, Clone)]
pub struct Backoff {
    initial: Duration,
    max: Duration,
    current: Duration,
}

impl Backoff {
    /// Creates a backoff starting at `initial` and capped at `max`.
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            current: initial,
        }
    }

    /// Returns the delay to wait now and doubles the next one.
    pub fn next(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        delay
    }

    /// Starts the sequence over from the initial delay.
    pub fn reset(&mut self) {
        self.current = self.initial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_until_the_cap() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(5));
        let delays: Vec<_> = (0..5).map(|_| backoff.next().as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 5, 5]);
    }

    #[test]
    fn reset_starts_over() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
        backoff.next();
        backoff.next();
        backoff.reset();
        assert_eq!(backoff.next(), Duration::from_secs(1));
    }
}
