//! Seeded shuffles for bulk "surprise me" edits.
//!
//! The results are concrete per-clip values that go into
//! `Command::SetTransitionEach` / `SetMotionEach`, so undo and redo replay
//! exactly what the user saw. The seed comes from the caller (the app uses
//! the clock), which keeps this module pure and testable.

use crate::project::{Motion, Transition, TransitionKind};
use crate::time::Ticks;

/// SplitMix64: tiny, fast, good enough for picking list items.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform index in `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        // Modulo bias is irrelevant for lists of a dozen items.
        #[allow(clippy::cast_possible_truncation)]
        let v = (self.next_u64() % n as u64) as usize;
        v
    }
}

/// Picks from `pool` for each position so that no two consecutive picks are
/// equal (when the pool has more than one entry). `previous` is the value
/// just before the first position, if any, so the rule also holds at the
/// start of the run.
fn pick_run<T: Copy + PartialEq>(
    pool: &[T],
    count: usize,
    previous: Option<T>,
    rng: &mut Rng,
) -> Vec<T> {
    let mut out = Vec::with_capacity(count);
    let mut last = previous;
    for _ in 0..count {
        let mut v = pool[rng.below(pool.len())];
        if pool.len() > 1 {
            while Some(v) == last {
                v = pool[rng.below(pool.len())];
            }
        }
        out.push(v);
        last = Some(v);
    }
    out
}

/// A varied transition for each index, all with `duration`. Consecutive
/// indices never get the same kind.
#[must_use]
pub fn transitions(indices: &[usize], duration: Ticks, seed: u64) -> Vec<(usize, Transition)> {
    let mut rng = Rng::new(seed);
    let kinds = pick_run(&TransitionKind::SHUFFLE_POOL, indices.len(), None, &mut rng);
    indices
        .iter()
        .copied()
        .zip(kinds)
        .map(|(i, kind)| (i, Transition { kind, duration }))
        .collect()
}

/// A varied movement for each index. Consecutive indices never get the same
/// preset.
#[must_use]
pub fn motions(indices: &[usize], seed: u64) -> Vec<(usize, Motion)> {
    let mut rng = Rng::new(seed ^ 0xA5A5_A5A5_A5A5_A5A5);
    let presets = pick_run(&Motion::SHUFFLE_POOL, indices.len(), None, &mut rng);
    indices.iter().copied().zip(presets).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn same_seed_same_result_different_seed_different_result() {
        let idx: Vec<usize> = (0..40).collect();
        let a = transitions(&idx, Ticks::SECOND, 7);
        assert_eq!(a, transitions(&idx, Ticks::SECOND, 7));
        assert_ne!(a, transitions(&idx, Ticks::SECOND, 8));
        assert_eq!(motions(&idx, 3), motions(&idx, 3));
    }

    #[test]
    fn uses_the_whole_pool_over_a_long_run() {
        let idx: Vec<usize> = (0..300).collect();
        let kinds: std::collections::HashSet<_> = transitions(&idx, Ticks::SECOND, 1)
            .into_iter()
            .map(|(_, t)| t.kind)
            .collect();
        assert_eq!(kinds.len(), TransitionKind::SHUFFLE_POOL.len());
        assert!(!kinds.contains(&TransitionKind::Cut));
        let moves: std::collections::HashSet<_> =
            motions(&idx, 1).into_iter().map(|(_, m)| m).collect();
        assert_eq!(moves.len(), Motion::SHUFFLE_POOL.len());
        assert!(!moves.contains(&Motion::None));
    }

    #[test]
    fn empty_input_gives_empty_output() {
        assert!(transitions(&[], Ticks::SECOND, 1).is_empty());
        assert!(motions(&[], 1).is_empty());
    }

    proptest! {
        #[test]
        fn neighbours_never_repeat(seed in any::<u64>(), n in 0usize..60) {
            let idx: Vec<usize> = (0..n).map(|i| i * 2).collect();
            let t = transitions(&idx, Ticks::from_millis(700), seed);
            prop_assert_eq!(t.len(), n);
            prop_assert!(t.iter().map(|(i, _)| *i).eq(idx.iter().copied()), "indices preserved in order");
            prop_assert!(t.iter().all(|(_, tr)| tr.duration == Ticks::from_millis(700)));
            prop_assert!(t.windows(2).all(|w| w[0].1.kind != w[1].1.kind));
            let m = motions(&idx, seed);
            prop_assert!(m.windows(2).all(|w| w[0].1 != w[1].1));
        }
    }
}
