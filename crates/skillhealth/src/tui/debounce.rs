use std::time::{Duration, Instant};

/// Quiet-period debouncer: `on_event` (re)arms the deadline; `should_fire`
/// returns true once, after `delay` has elapsed with no further events.
/// Pure state machine — callers feed it `Instant`s, so tests never sleep.
pub struct Debouncer {
    delay: Duration,
    deadline: Option<Instant>,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Debouncer {
            delay,
            deadline: None,
        }
    }

    pub fn on_event(&mut self, now: Instant) {
        self.deadline = Some(now + self.delay);
    }

    pub fn should_fire(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(d) if now >= d => {
                self.deadline = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn fires_only_after_quiet_period() {
        let mut d = Debouncer::new(Duration::from_millis(500));
        let t0 = Instant::now();
        assert!(!d.should_fire(t0)); // nothing pending
        d.on_event(t0);
        assert!(!d.should_fire(t0 + Duration::from_millis(499)));
        assert!(d.should_fire(t0 + Duration::from_millis(500)));
        // fired → resets
        assert!(!d.should_fire(t0 + Duration::from_millis(600)));
    }

    #[test]
    fn new_events_push_the_deadline() {
        let mut d = Debouncer::new(Duration::from_millis(500));
        let t0 = Instant::now();
        d.on_event(t0);
        d.on_event(t0 + Duration::from_millis(400)); // burst continues
        assert!(!d.should_fire(t0 + Duration::from_millis(500)));
        assert!(d.should_fire(t0 + Duration::from_millis(900)));
    }
}
