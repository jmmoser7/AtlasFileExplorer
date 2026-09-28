//! Nested rails: how wires that share one port fan out without crossing.
//!
//! The File Atlas leader rule, shared by its folder map and Slate's square
//! wires. Each wire's far end sits on one side of the port,
//! measured along the port edge. Per side, the farthest wire exits
//! outermost along the edge and turns nearest the port; each nearer wire
//! exits one step inside and turns one step further out, so no two cross.
//!
//! Spacing is best effort (user, 28 September 2026): each connection has a
//! bounded zone; lanes spread at a preferred spacing while they fit, then
//! pack evenly denser inside the same zone, down to a floor. The zone never
//! grows and no wire is sent far away to keep its spacing.

/// How far apart parallel lanes sit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneSpacing {
    /// Spacing while the lanes fit.
    pub preferred: f32,
    /// The densest packing. Past it lanes may leave the room.
    pub min: f32,
}

impl LaneSpacing {
    /// Even spacing for `steps` lane steps inside `room`: `preferred` while
    /// it fits, then `room / steps`, never below `min`.
    pub fn fit(&self, steps: usize, room: f32) -> f32 {
        if steps == 0 {
            return self.preferred;
        }
        self.preferred.min(room / steps as f32).max(self.min)
    }
}

/// Where wires arriving at one side of a node spread out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionZone {
    /// Along the edge, centred on the port. Clipped to the edge.
    pub width: f32,
    /// Out from the edge: the room trunks turn in, past the clearance.
    pub depth: f32,
    /// Lane spacing, both along the edge and out from it.
    pub spacing: LaneSpacing,
}

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

    #[test]
    fn lanes_keep_their_spacing_then_pack_evenly_to_the_floor() {
        let s = LaneSpacing {
            preferred: 10.0,
            min: 3.0,
        };
        assert_eq!(s.fit(0, 0.0), 10.0, "a lone lane keeps the preferred");
        assert_eq!(s.fit(3, 48.0), 10.0, "fits: preferred spacing");
        assert_eq!(s.fit(6, 48.0), 8.0, "full: denser, same room");
        assert_eq!(s.fit(40, 48.0), 3.0, "never below the floor");
    }
}
