//! Resident strip pixels are bounded independently of the clip count.
use super::*;

impl VideoBoard {
    pub(super) fn strip_bytes(&self) -> usize {
        self.strips
            .values()
            .flat_map(|strip| strip.slots.iter().flatten())
            .map(|slot| slot.rgba.len())
            .sum()
    }

    pub(super) fn reserve_strip_frame(
        &mut self,
        keep: NodeId,
        index: usize,
        bytes: usize,
        budget: usize,
    ) -> bool {
        let replaced = self
            .strips
            .get(&keep)
            .and_then(|strip| strip.slots.get(index))
            .and_then(Option::as_ref)
            .map_or(0, |slot| slot.rgba.len());
        while self
            .strip_bytes()
            .saturating_sub(replaced)
            .saturating_add(bytes)
            > budget
        {
            let victim = self.lru.iter().copied().find(|id| {
                *id != keep
                    && self
                        .strips
                        .get(id)
                        .is_some_and(|strip| strip.slots.iter().any(Option::is_some))
            });
            let Some(victim) = victim else { return false };
            let strip = self.strips.get_mut(&victim).unwrap();
            // Retain its probe/trim identity. A later hover requests all sample
            // positions again; delayed events from this strip cannot refill it.
            strip.slots.iter_mut().for_each(|slot| *slot = None);
            strip.token = 0;
            strip.done = false;
            strip.inflight = false;
            if !self.play.as_ref().is_some_and(|play| play.id == victim) {
                self.textures.remove(&victim);
            }
        }
        true
    }
}
