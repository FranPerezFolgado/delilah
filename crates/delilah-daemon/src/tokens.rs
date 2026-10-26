//! The running token count of one session's transcript: where the next read starts,
//! what was counted so far and the last message id counted (see `transcript_watch.rs`).

use delilah_agents::transcript::{TokenUsage, UsageDelta};

#[derive(Debug, Clone, Default)]
pub struct TokenMeter {
    pub offset: u64,
    pub usage: TokenUsage,
    pub last_id: Option<String>,
}

impl TokenMeter {
    /// Adds what was read from `from`. Does nothing (false) if the meter moved since;
    /// otherwise returns whether the total changed.
    pub fn apply(&mut self, from: u64, delta: UsageDelta) -> bool {
        if self.offset != from {
            return false;
        }
        let before = self.usage.total;
        self.usage.add(&delta.usage);
        self.offset += delta.consumed as u64;
        self.last_id = delta.last_id;
        self.usage.total != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(total: u64, consumed: usize, id: &str) -> UsageDelta {
        UsageDelta {
            usage: TokenUsage { total, input: total, ..Default::default() },
            consumed,
            last_id: Some(id.into()),
        }
    }

    #[test]
    fn apply_adds_and_advances() {
        let mut m = TokenMeter::default();
        assert!(m.apply(0, delta(10, 100, "a")));
        assert_eq!((m.offset, m.usage.total, m.last_id.as_deref()), (100, 10, Some("a")));
        assert!(!m.apply(100, delta(0, 20, "a")));
        assert_eq!(m.offset, 120);
    }

    #[test]
    fn apply_ignores_a_stale_read() {
        let mut m = TokenMeter::default();
        assert!(m.apply(0, delta(10, 100, "a")));
        assert!(!m.apply(0, delta(10, 100, "a")));
        assert_eq!((m.offset, m.usage.total), (100, 10));
    }
}
