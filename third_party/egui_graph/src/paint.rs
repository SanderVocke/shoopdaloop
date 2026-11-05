//! Geometry helpers for painting edge paths in custom ways, e.g. as
//! multiple parallel strands via [`parallel_polylines`].
//!
//! Dashed and dotted painting need no helpers here - see
//! [`egui::Shape::dashed_line`] and friends, which accept the same point
//! lists (dotted lines are dashed lines with a dash length close to the
//! stroke width).

/// Offset every point of a polyline perpendicular to its local direction.
///
/// The local direction at a point is the average of its adjacent segment
/// directions (the segment direction itself at the endpoints). A positive
/// `offset` displaces along that direction rotated 90 degrees
/// counter-clockwise (in egui's y-down space).
///
/// Degenerate inputs are returned unchanged: fewer than two points, a zero
/// `offset`, or points whose local direction vanishes (coincident
/// neighbours, hairpin corners) keep their position.
pub fn offset_polyline(points: &[egui::Pos2], offset: f32) -> Vec<egui::Pos2> {
    if offset == 0.0 || points.len() < 2 {
        return points.to_vec();
    }
    let dir = |a: egui::Pos2, b: egui::Pos2| (b - a).normalized();
    (0..points.len())
        .map(|i| {
            let prev = (i > 0).then(|| points[i - 1]);
            let next = points.get(i + 1).copied();
            let d = match (prev, next) {
                (Some(p), Some(n)) => (dir(p, points[i]) + dir(points[i], n)).normalized(),
                (None, Some(n)) => dir(points[i], n),
                (Some(p), None) => dir(p, points[i]),
                (None, None) => egui::Vec2::ZERO,
            };
            let perp = egui::vec2(-d.y, d.x);
            points[i] + perp * offset
        })
        .collect()
}

/// `n` parallel polylines centred on `points`, adjacent strands `spacing`
/// apart (strand `i` is [`offset_polyline`] by `(i - (n - 1) / 2) * spacing`).
///
/// `n = 1` yields the input polyline unchanged. `n = 0` yields no polylines.
pub fn parallel_polylines(points: &[egui::Pos2], n: usize, spacing: f32) -> Vec<Vec<egui::Pos2>> {
    (0..n)
        .map(|i| {
            let offset = (i as f32 - (n as f32 - 1.0) * 0.5) * spacing;
            offset_polyline(points, offset)
        })
        .collect()
}

/// Short tick marks crossing `points` at every `spacing` graph units, for a
/// striped-cord look. Each tick is `length` long, centred on the path and
/// leaning `lean` radians off perpendicular (`0.0` is a square tick;
/// positive tilts toward the path's direction of travel for a forward
/// slash).
///
/// Returns one `[start, end]` segment per tick. The first tick sits half a
/// `spacing` in, so ticks never crowd the sockets. Degenerate inputs (fewer
/// than two points, non-positive `spacing`) yield no ticks.
pub fn hatch_marks(
    points: &[egui::Pos2],
    spacing: f32,
    length: f32,
    lean: f32,
) -> Vec<[egui::Pos2; 2]> {
    let mut ticks = Vec::new();
    if points.len() < 2 || spacing <= 0.0 {
        return ticks;
    }
    let half = length * 0.5;
    let (sin, cos) = lean.sin_cos();
    let mut acc = 0.0;
    let mut next = spacing * 0.5;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = b - a;
        let seg_len = seg.length();
        if seg_len <= f32::EPSILON {
            continue;
        }
        let dir = seg / seg_len;
        // The tick direction: the path normal (90 deg CCW), leaned toward the
        // travel direction by `lean`.
        let perp = egui::vec2(-dir.y, dir.x);
        let tick = perp * cos + dir * sin;
        while next <= acc + seg_len {
            let center = a + seg * ((next - acc) / seg_len);
            ticks.push([center - tick * half, center + tick * half]);
            next += spacing;
        }
        acc += seg_len;
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, Pos2};

    fn assert_pos_eq(a: Pos2, b: Pos2) {
        assert!((a - b).length() < 1e-4, "{a:?} != {b:?}");
    }

    #[test]
    fn offset_zero_and_degenerate_inputs_unchanged() {
        let pts = [pos2(0.0, 0.0), pos2(10.0, 0.0)];
        assert_eq!(offset_polyline(&pts, 0.0), pts.to_vec());
        assert_eq!(offset_polyline(&[], 2.0), Vec::<Pos2>::new());
        assert_eq!(offset_polyline(&pts[..1], 2.0), pts[..1].to_vec());
    }

    #[test]
    fn offset_straight_line_shifts_perpendicular() {
        // Rightward line: perpendicular (90 degrees CCW in y-down space) of
        // (1, 0) is (0, 1).
        let pts = [pos2(0.0, 0.0), pos2(5.0, 0.0), pos2(10.0, 0.0)];
        let off = offset_polyline(&pts, 2.0);
        for (o, p) in off.iter().zip(&pts) {
            assert_pos_eq(*o, *p + egui::vec2(0.0, 2.0));
        }
    }

    #[test]
    fn offset_right_angle_corner_averages_directions() {
        // Right then down: the corner's local direction is the normalized
        // average of (1, 0) and (0, 1).
        let pts = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        let off = offset_polyline(&pts, 2.0);
        assert_pos_eq(off[0], pos2(0.0, 2.0));
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert_pos_eq(off[1], pts[1] + egui::vec2(-d, d) * 2.0);
        assert_pos_eq(off[2], pos2(8.0, 10.0));
    }

    #[test]
    fn parallel_polylines_centre_and_symmetry() {
        let pts = [pos2(0.0, 0.0), pos2(10.0, 0.0)];
        assert_eq!(parallel_polylines(&pts, 0, 3.0), Vec::<Vec<Pos2>>::new());
        assert_eq!(parallel_polylines(&pts, 1, 3.0), vec![pts.to_vec()]);
        let strands = parallel_polylines(&pts, 3, 3.0);
        assert_eq!(strands.len(), 3);
        assert_eq!(strands[1], pts.to_vec());
        for (a, b) in strands[0].iter().zip(&strands[2]) {
            // Outer strands sit symmetrically about the centre.
            assert_pos_eq(pos2(a.x, -a.y), pos2(b.x, b.y));
        }
    }

    #[test]
    fn hatch_marks_space_and_orient() {
        // A rightward line, length 30. First tick at spacing/2 = 5, then every
        // 10, so at 5, 15, 25 - three ticks.
        let pts = [pos2(0.0, 0.0), pos2(30.0, 0.0)];
        let ticks = hatch_marks(&pts, 10.0, 8.0, 0.0);
        assert_eq!(ticks.len(), 3);
        for (i, [a, b]) in ticks.iter().enumerate() {
            let x = 5.0 + i as f32 * 10.0;
            // A square tick (lean 0) is perpendicular: vertical here, centred
            // on the path, `length` tall.
            assert_pos_eq(*a, pos2(x, -4.0));
            assert_pos_eq(*b, pos2(x, 4.0));
        }
        // A forward lean tilts the tick toward the travel direction.
        let leaned = hatch_marks(&pts, 10.0, 8.0, 0.5);
        assert!(leaned[0][1].x > leaned[0][0].x, "top end leans forward");
    }

    #[test]
    fn hatch_marks_degenerate_inputs_empty() {
        let pts = [pos2(0.0, 0.0), pos2(10.0, 0.0)];
        assert!(hatch_marks(&pts, 0.0, 8.0, 0.0).is_empty());
        assert!(hatch_marks(&pts[..1], 10.0, 8.0, 0.0).is_empty());
    }
}
