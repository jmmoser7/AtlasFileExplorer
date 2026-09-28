//! Nested rails: how wires that share one port fan out without crossing.
//!
//! The File Atlas leader rule, shared by its folder map and Slate's
//! crosstalk wires. Each wire's far end sits on one side of the port,
//! measured along the port edge. Per side, the farthest wire exits
//! outermost along the edge and turns nearest the port; each nearer wire
//! exits one step inside and turns one step further out, so no two cross.

/// One wire's place in its port's fan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rail {
    /// `1.0` or `-1.0`: the side of the port its far end sits on.
    pub side: f32,
    /// `0` for the farthest wire on its side, turning nearest the port.
    pub rank: usize,
    /// Wires on this side of the port.
    pub count: usize,
}

impl Rail {
    /// Where the wire leaves the port, along the edge: the farthest wire
    /// exits outermost, `count` steps of `gap` from the centre.
    pub fn exit(&self, gap: f32) -> f32 {
        self.side * (self.count - self.rank) as f32 * gap
    }
}

/// Ranks wires sharing one port. `offsets[i]` is wire `i`'s far end along
/// the port edge, signed, from the port centre. A wire within `dead_zone`
/// of the centre runs straight (`None`), so its side never flips while it
/// is dragged across. Equal offsets keep their input order.
pub fn nested_rails(offsets: &[f32], dead_zone: f32) -> Vec<Option<Rail>> {
    let mut out = vec![None; offsets.len()];
    for side in [-1.0f32, 1.0] {
        let mut list: Vec<(f32, usize)> = offsets
            .iter()
            .enumerate()
            .map(|(i, o)| (o * side, i))
            .filter(|(far, _)| *far > dead_zone)
            .collect();
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
        let count = list.len();
        for (rank, &(_, i)) in list.iter().enumerate() {
            out[i] = Some(Rail { side, rank, count });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_farthest_wire_exits_outermost_and_turns_first() {
        let rails = nested_rails(&[-300.0, -100.0, 0.2, 80.0, 240.0], 0.5);
        assert_eq!(rails[2], None, "a centred wire runs straight");
        let far = rails[0].unwrap();
        let near = rails[1].unwrap();
        assert_eq!((far.side, far.rank, far.count), (-1.0, 0, 2));
        assert_eq!((near.side, near.rank, near.count), (-1.0, 1, 2));
        assert!(far.exit(4.0) < near.exit(4.0), "outermost on its side");
        assert_eq!(rails[4].unwrap().rank, 0);
        assert_eq!(rails[3].unwrap().exit(4.0), 4.0);
        assert_eq!(rails[4].unwrap().exit(4.0), 8.0);
    }

    #[test]
    fn equal_offsets_keep_their_order() {
        let rails = nested_rails(&[50.0, 50.0], 12.0);
        assert_eq!(rails[0].unwrap().rank, 0);
        assert_eq!(rails[1].unwrap().rank, 1);
        assert_eq!(nested_rails(&[10.0, -12.0], 12.0), vec![None, None]);
    }
}
