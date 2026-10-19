//! Event-time selection for random agent departures.
use super::art;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

pub const RANDOM_DEPARTURE: &str = "random";

/// Ephemeral shuffle-bag state. The art catalogue remains the sole source of ids.
#[derive(Default)]
pub struct DepartureSequence {
    random: RandomState,
    counter: u64,
    pending: Vec<String>,
    last: Option<String>,
}

impl DepartureSequence {
    /// Resolves a departure when the event is created, never during painting.
    pub fn resolve(&mut self, requested: &str, portal_available: bool) -> String {
        if requested != RANDOM_DEPARTURE {
            self.pending.retain(|id| id != requested);
            self.last = Some(requested.to_owned());
            return requested.to_owned();
        }

        let eligible: Vec<String> = art::master()
            .departures
            .iter()
            .filter(|departure| portal_available || departure.id != "portal")
            .map(|departure| departure.id.clone())
            .collect();
        self.pending
            .retain(|id| eligible.iter().any(|candidate| candidate == id));

        if self.pending.is_empty() {
            self.pending.clone_from(&eligible);
            self.shuffle();
            if self.pending.len() > 1 && self.pending.first() == self.last.as_ref() {
                self.pending.swap(0, 1);
            }
        }

        let choice = if self.pending.is_empty() {
            // The catalogue is expected to be non-empty, but preserve the
            // requested sentinel if a future empty catalogue makes this fail.
            RANDOM_DEPARTURE.to_owned()
        } else {
            self.pending.remove(0)
        };
        self.last = Some(choice.clone());
        choice
    }

    fn shuffle(&mut self) {
        for index in (1..self.pending.len()).rev() {
            let slot = (self.random.hash_one(self.counter) as usize) % (index + 1);
            self.counter = self.counter.wrapping_add(1);
            self.pending.swap(index, slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(portal_available: bool) -> Vec<String> {
        art::master()
            .departures
            .iter()
            .filter(|departure| portal_available || departure.id != "portal")
            .map(|departure| departure.id.clone())
            .collect()
    }

    #[test]
    fn complete_cycles_cover_each_eligible_style_once_without_adjacent_repeats() {
        let expected = ids(true);
        let mut sequence = DepartureSequence::default();
        let choices: Vec<_> = (0..expected.len() * 3)
            .map(|_| sequence.resolve(RANDOM_DEPARTURE, true))
            .collect();

        for cycle in choices.chunks_exact(expected.len()) {
            let mut actual = cycle.to_vec();
            let mut sorted_expected = expected.clone();
            actual.sort();
            sorted_expected.sort();
            assert_eq!(actual, sorted_expected);
        }
        assert!(choices.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn portal_eligibility_changes_apply_to_the_remaining_bag() {
        // Start with a pending portal, then remove the visible portal before
        // the next farewell. Selection must purge that now-ineligible exit.
        let mut sequence = DepartureSequence {
            pending: ids(true),
            ..Default::default()
        };
        let without_portal = ids(false);
        let first_cycle: Vec<_> = (0..without_portal.len())
            .map(|_| sequence.resolve(RANDOM_DEPARTURE, false))
            .collect();
        assert!(!first_cycle.iter().any(|id| id == "portal"));
        let mut sorted_cycle = first_cycle;
        sorted_cycle.sort();
        let mut sorted_expected = without_portal;
        sorted_expected.sort();
        assert_eq!(sorted_cycle, sorted_expected);

        let mut saw_portal = false;
        for _ in 0..ids(true).len() * 2 {
            saw_portal |= sequence.resolve(RANDOM_DEPARTURE, true) == "portal";
        }
        assert!(saw_portal);
    }

    #[test]
    fn explicit_choice_is_preserved_and_removed_from_the_next_random_cycle() {
        let mut sequence = DepartureSequence::default();
        let explicit = ids(true)[0].clone();
        assert_eq!(sequence.resolve(&explicit, true), explicit);
        assert_ne!(sequence.resolve(RANDOM_DEPARTURE, true), explicit);
    }
}
