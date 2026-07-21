use std::time::{Duration, Instant};

const DEFAULT_SAFETY_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct WatchCoordinator {
    next_generation: u64,
    latest_requested: u64,
    in_flight: Option<u64>,
    dirty: bool,
    quiet_delay: Duration,
    maximum_delay: Duration,
    quiet_deadline: Option<Instant>,
    maximum_deadline: Option<Instant>,
    safety_deadline: Instant,
    safety_interval: Duration,
}

impl WatchCoordinator {
    pub fn new(start: Instant, quiet_delay: Duration, maximum_delay: Duration) -> Self {
        Self::with_safety_interval(start, quiet_delay, maximum_delay, DEFAULT_SAFETY_INTERVAL)
    }

    pub fn with_safety_interval(
        start: Instant,
        quiet_delay: Duration,
        maximum_delay: Duration,
        safety_interval: Duration,
    ) -> Self {
        Self {
            next_generation: 0,
            latest_requested: 0,
            in_flight: None,
            dirty: false,
            quiet_delay,
            maximum_delay,
            quiet_deadline: None,
            maximum_deadline: None,
            safety_deadline: start + safety_interval,
            safety_interval,
        }
    }

    pub fn event_hint(&mut self, now: Instant) {
        if self.in_flight.is_some() {
            self.dirty = true;
            return;
        }
        self.quiet_deadline = Some(now + self.quiet_delay);
        self.maximum_deadline
            .get_or_insert(now + self.maximum_delay);
    }

    pub fn manual_hint(&mut self, now: Instant) {
        if self.in_flight.is_some() {
            self.dirty = true;
            return;
        }
        self.quiet_deadline = Some(now);
        self.maximum_deadline = Some(now);
    }

    pub fn tick(&mut self, now: Instant) -> Option<u64> {
        if self.in_flight.is_some() {
            return None;
        }
        let event_due = self.quiet_deadline.is_some_and(|deadline| now >= deadline)
            || self
                .maximum_deadline
                .is_some_and(|deadline| now >= deadline);
        if !event_due && now < self.safety_deadline {
            return None;
        }
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_requested = self.next_generation;
        self.in_flight = Some(self.next_generation);
        self.quiet_deadline = None;
        self.maximum_deadline = None;
        Some(self.next_generation)
    }

    pub fn finish(&mut self, generation: u64, now: Instant) {
        if self.in_flight != Some(generation) {
            return;
        }
        self.in_flight = None;
        self.safety_deadline = now + self.safety_interval;
        if self.dirty {
            self.dirty = false;
            self.quiet_deadline = Some(now);
            self.maximum_deadline = Some(now);
        }
    }

    pub fn accept_result(&self, generation: u64) -> bool {
        self.in_flight == Some(generation) && generation == self.latest_requested
    }
}
