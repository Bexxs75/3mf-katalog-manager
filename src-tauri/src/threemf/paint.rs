//! Decodes the per-triangle paint strings that Bambu Studio and OrcaSlicer
//! write as `paint_color` (PrusaSlicer: `slic3rpe:mmu_segmentation`).
//!
//! The format comes from the slicers' `TriangleSelector` and
//! `FacetsAnnotation::get_triangle_as_string`: the hex digits are read from
//! the last to the first, each digit is one 4-bit code. A code's low two bits
//! hold the number of split sides. A leaf (0 split sides) keeps its state in
//! the upper two bits; the value 3 there means the next digit holds
//! `state - 3`, and a next digit of 0xF means one more digit holds
//! `state - 18`. A split node's upper bits name the special side, and its
//! 2, 3 or 4 children follow depth-first in reverse child order. State 0 is
//! unpainted (the part's own filament), state n paints with filament n.
//!
//! The preview keeps the original mesh, so a subdivided triangle cannot show
//! its sub-triangles. It gets the state covering the largest share of its
//! area instead. Splits always cut at edge midpoints, which makes the child
//! area shares exact for any triangle shape.

/// Longer strings are not plausible brush detail and are treated as damaged.
const MAX_PAINT_DIGITS: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintState {
    /// Filament number covering most of the triangle; 0 = unpainted.
    pub state: u8,
    /// The triangle was subdivided, so `state` simplifies it.
    pub split: bool,
}

/// `None` for an empty, overlong or malformed string.
pub fn dominant_state(code: &str) -> Option<PaintState> {
    let bytes = code.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_PAINT_DIGITS || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let mut digits = bytes.iter().rev().map(|b| (*b as char).to_digit(16).unwrap_or(0) as u8);
    let mut areas = [0.0f64; 64];
    let mut split = false;
    // Area share of each node still to be read, last-read child on top.
    let mut pending = vec![1.0f64];
    while let Some(weight) = pending.pop() {
        let code = digits.next()?;
        let sides = code & 0b11;
        if sides == 0 {
            let state = if code & 0b1100 == 0b1100 {
                match digits.next()? {
                    0b1111 => digits.next()? + 18,
                    low => low + 3,
                }
            } else {
                code >> 2
            };
            areas[usize::from(state)] += weight;
            continue;
        }
        // The special side is 0..=2; only a three-side split ignores it.
        if sides != 3 && code >> 2 == 3 {
            return None;
        }
        split = true;
        // Children are stored last child first. One split halves the
        // triangle; two splits give the corner child and its neighbour a
        // quarter each and the last child half; three splits give quarters.
        let shares: &[f64] = match sides {
            1 => &[0.5, 0.5],
            2 => &[0.5, 0.25, 0.25],
            _ => &[0.25; 4],
        };
        pending.extend(shares.iter().rev().map(|share| weight * share));
    }
    let mut state = 0;
    for (candidate, area) in areas.iter().enumerate() {
        if *area > areas[state] {
            state = candidate;
        }
    }
    Some(PaintState { state: state as u8, split })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(state: u8) -> PaintState {
        PaintState { state, split: false }
    }

    #[test]
    fn single_digit_leaves_store_states_one_and_two() {
        assert_eq!(dominant_state("4"), Some(leaf(1)));
        assert_eq!(dominant_state("8"), Some(leaf(2)));
        assert_eq!(dominant_state("0"), Some(leaf(0)));
    }

    #[test]
    fn extended_leaves_read_the_next_digit_from_the_right() {
        // Values seen in real Bambu Studio projects.
        assert_eq!(dominant_state("0C"), Some(leaf(3)));
        assert_eq!(dominant_state("1C"), Some(leaf(4)));
        assert_eq!(dominant_state("CC"), Some(leaf(15)));
        assert_eq!(dominant_state("0FC"), Some(leaf(18)));
        assert_eq!(dominant_state("FFC"), Some(leaf(33)));
        assert_eq!(dominant_state("1c"), Some(leaf(4)));
    }

    #[test]
    fn three_side_split_picks_the_state_with_most_quarters() {
        // Root 0x3, then four leaves (read right to left): 8, 8, 4, 0.
        assert_eq!(dominant_state("04883"), Some(PaintState { state: 2, split: true }));
    }

    #[test]
    fn two_side_split_weights_the_first_stored_child_as_half() {
        // Root: two split sides, special side 0 (0x2). Children in reading
        // order: half (state 1), quarter (state 2), quarter (state 2). The
        // tie between one half and two quarters keeps the lower state.
        assert_eq!(dominant_state("8842"), Some(PaintState { state: 1, split: true }));
        // Half with state 2 wins over two quarters of different states.
        assert_eq!(dominant_state("0C482"), Some(PaintState { state: 2, split: true }));
    }

    #[test]
    fn nested_splits_multiply_area_shares() {
        // Root halves (0x1): first child splits again into halves of states
        // 1 and 2 (0x1, 4, 8), second child is state 2 (8): state 2 = 3/4.
        assert_eq!(dominant_state("88411"), Some(PaintState { state: 2, split: true }));
    }

    #[test]
    fn rejects_damaged_strings() {
        for code in ["", "G", "3", "C", "0F", "D", "2", "#4", " 4"] {
            assert_eq!(dominant_state(code), None, "{code}");
        }
        assert_eq!(dominant_state(&"4".repeat(MAX_PAINT_DIGITS + 1)), None);
    }
}
