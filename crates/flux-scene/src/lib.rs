//! Immutable logical scene geometry, already transformed into one viewport's
//! coordinates. This crate does not own camera transforms, surfaces or GPU work.
use flux_core::{Point, Rect, WindowId};
use std::collections::HashSet;

pub const MAX_ENTRIES: usize = 1024;
/// Logical view-coordinate and endpoint bound; worst span is 2e9, below i32::MAX.
pub const MAX_COORD: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GeometryError {
    #[error("rectangle must have finite, positive, representable extents")]
    InvalidRect,
    #[error("scene coordinates or endpoints exceed the supported logical range")]
    CoordinateLimit,
    #[error("rounded rectangle edges or size exceed the i32 domain")]
    IntegerOverflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryField {
    VisualRect,
    ContainerClip,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SceneError {
    #[error("invalid viewport: {0}")]
    InvalidViewport(GeometryError),
    #[error("invalid {field:?} for window {window}: {source}")]
    InvalidEntry {
        window: WindowId,
        field: GeometryField,
        source: GeometryError,
    },
    #[error("window {0} appears more than once")]
    DuplicateWindow(WindowId),
    #[error("scene input exceeds the {MAX_ENTRIES}-entry limit")]
    CapacityExceeded,
}

/// Raw construction input, copied into a validated immutable snapshot.
/// Both rectangles are logical viewport coordinates, including their origins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneInput {
    pub window: WindowId,
    pub visual_rect: Rect,
    pub container_clip: Rect,
    /// Ascending draw order. Equal z values preserve input order.
    pub z_index: i32,
    /// Metadata only: focus does not implicitly override the explicit z order.
    pub focused: bool,
    pub visible: bool,
}
impl SceneInput {
    pub fn new(window: WindowId, visual_rect: Rect, container_clip: Rect) -> Self {
        Self {
            window,
            visual_rect,
            container_clip,
            z_index: 0,
            focused: false,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ValidRect {
    rect: Rect,
    right: f64,
    bottom: f64,
}
impl ValidRect {
    fn new(rect: Rect) -> Result<Self, GeometryError> {
        let right = rect.x + rect.width;
        let bottom = rect.y + rect.height;
        if ![rect.x, rect.y, rect.width, rect.height, right, bottom]
            .into_iter()
            .all(f64::is_finite)
            || rect.width <= 0.0
            || rect.height <= 0.0
            || right <= rect.x
            || bottom <= rect.y
        {
            return Err(GeometryError::InvalidRect);
        }
        Ok(Self {
            rect,
            right,
            bottom,
        })
    }
    fn scene(rect: Rect) -> Result<Self, GeometryError> {
        let valid = Self::new(rect)?;
        if [rect.x, rect.y, valid.right, valid.bottom]
            .into_iter()
            .any(|value| value.abs() > MAX_COORD)
        {
            return Err(GeometryError::CoordinateLimit);
        }
        Ok(valid)
    }
    fn intersect(self, other: Self) -> Option<Self> {
        let left = self.rect.x.max(other.rect.x);
        let top = self.rect.y.max(other.rect.y);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        if right <= left || bottom <= top {
            return None;
        }
        // Edges lie inside both validated positive intervals. Keep those exact
        // edges for picking instead of recomputing them from a rounded width.
        Some(Self {
            rect: Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            },
            right,
            bottom,
        })
    }
    fn contains(self, point: Point) -> bool {
        point.x >= self.rect.x
            && point.y >= self.rect.y
            && point.x < self.right
            && point.y < self.bottom
    }
}

/// Strict finite positive intersection; touching half-open edges have no area.
pub fn intersection(first: Rect, second: Rect) -> Result<Option<Rect>, GeometryError> {
    Ok(ValidRect::new(first)?
        .intersect(ValidRect::new(second)?)
        .map(|rect| rect.rect))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneEntry {
    window: WindowId,
    visual: ValidRect,
    container: ValidRect,
    effective_clip: ValidRect,
    visible: ValidRect,
    z_index: i32,
    focused: bool,
}
impl SceneEntry {
    pub fn window(&self) -> WindowId {
        self.window
    }
    /// Actual window geometry; never resized to fit its allocation/container.
    pub fn visual_rect(&self) -> Rect {
        self.visual.rect
    }
    pub fn container_clip(&self) -> Rect {
        self.container.rect
    }
    /// Container intersected with the viewport; may extend past the visual rect.
    pub fn effective_clip(&self) -> Rect {
        self.effective_clip.rect
    }
    /// Visual rect intersected with container and viewport.
    pub fn visible_rect(&self) -> Rect {
        self.visible.rect
    }
    pub fn z_index(&self) -> i32 {
        self.z_index
    }
    pub fn focused(&self) -> bool {
        self.focused
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshot {
    viewport: Rect,
    entries: Box<[SceneEntry]>,
}
impl SceneSnapshot {
    /// Counts and validates every input, including hidden/offscreen entries.
    /// Duplicate identities are invalid even when only one duplicate is visible.
    /// No partially built snapshot escapes on failure.
    pub fn new(
        viewport: Rect,
        inputs: impl IntoIterator<Item = SceneInput>,
    ) -> Result<Self, SceneError> {
        let viewport_valid = ValidRect::scene(viewport).map_err(SceneError::InvalidViewport)?;
        let mut entries = Vec::new();
        let mut identities = HashSet::new();
        for (index, input) in inputs.into_iter().enumerate() {
            if index >= MAX_ENTRIES {
                return Err(SceneError::CapacityExceeded);
            }
            if !identities.insert(input.window) {
                return Err(SceneError::DuplicateWindow(input.window));
            }
            let visual =
                ValidRect::scene(input.visual_rect).map_err(|source| SceneError::InvalidEntry {
                    window: input.window,
                    field: GeometryField::VisualRect,
                    source,
                })?;
            let container = ValidRect::scene(input.container_clip).map_err(|source| {
                SceneError::InvalidEntry {
                    window: input.window,
                    field: GeometryField::ContainerClip,
                    source,
                }
            })?;
            if !input.visible {
                continue;
            }
            let Some(effective_clip) = container.intersect(viewport_valid) else {
                continue;
            };
            let Some(visible) = visual.intersect(effective_clip) else {
                continue;
            };
            entries.push(SceneEntry {
                window: input.window,
                visual,
                container,
                effective_clip,
                visible,
                z_index: input.z_index,
                focused: input.focused,
            });
        }
        entries.sort_by_key(|entry| entry.z_index);
        Ok(Self {
            viewport,
            entries: entries.into_boxed_slice(),
        })
    }
    pub fn viewport(&self) -> Rect {
        self.viewport
    }
    /// Back-to-front draw order. Returned storage has no mutation API.
    pub fn entries(&self) -> &[SceneEntry] {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Front-to-back rectangular hits, including visual and allocation clipping.
    /// These are candidates: actual Wayland surface input regions are external.
    pub fn hit_candidates(&self, point: Point) -> impl Iterator<Item = &SceneEntry> {
        let finite = point.x.is_finite() && point.y.is_finite();
        self.entries
            .iter()
            .rev()
            .filter(move |entry| finite && entry.visible.contains(point))
    }
    pub fn hit_test(&self, point: Point) -> Option<&SceneEntry> {
        self.hit_candidates(point).next()
    }
    /// Front-to-back owners whose allocation/viewport clip contains this point.
    /// Unlike hit_candidates, this permits surface-tree content outside the visual
    /// toplevel rectangle. Root must test actual surface input regions/subsurfaces.
    /// Popup policy is independent and may instead clip only to the viewport.
    pub fn clip_candidates(&self, point: Point) -> impl Iterator<Item = &SceneEntry> {
        let finite = point.x.is_finite() && point.y.is_finite();
        self.entries
            .iter()
            .rev()
            .filter(move |entry| finite && entry.effective_clip.contains(point))
    }
}

/// Integer rectangle emitted by round_rect_to_i32. Coordinates and extents use
/// i32 to match common logical renderer rectangles; the helper checks all edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegerRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Round all four edges independently to nearest integer, half away from zero.
/// Shared floating edges therefore stay shared; width/height are edge differences.
/// Positive subpixel rectangles that collapse return None. Overflow is never cast
/// with saturating Rust float-to-int semantics. No output/device scale is applied.
pub fn round_rect_to_i32(rect: Rect) -> Result<Option<IntegerRect>, GeometryError> {
    let valid = ValidRect::new(rect)?;
    fn edge(value: f64) -> Result<i32, GeometryError> {
        let rounded = value.round();
        if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
            return Err(GeometryError::IntegerOverflow);
        }
        Ok(rounded as i32)
    }
    let left = edge(rect.x)?;
    let top = edge(rect.y)?;
    let right = edge(valid.right)?;
    let bottom = edge(valid.bottom)?;
    let width = i64::from(right) - i64::from(left);
    let height = i64::from(bottom) - i64::from(top);
    if width == 0 || height == 0 {
        return Ok(None);
    }
    let width = i32::try_from(width).map_err(|_| GeometryError::IntegerOverflow)?;
    let height = i32::try_from(height).map_err(|_| GeometryError::IntegerOverflow)?;
    Ok(Some(IntegerRect {
        x: left,
        y: top,
        width,
        height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
    fn id(index: u32) -> WindowId {
        WindowId::new(index, 0)
    }
    fn input(index: u32, visual: Rect, clip: Rect) -> SceneInput {
        SceneInput::new(id(index), visual, clip)
    }
    fn ids(
        candidates: impl Iterator<Item = impl std::borrow::Borrow<SceneEntry>>,
    ) -> Vec<WindowId> {
        candidates.map(|entry| entry.borrow().window()).collect()
    }

    #[test]
    fn overlap_order_is_reverse_of_stable_explicit_draw_order() {
        let area = rect(0., 0., 100., 100.);
        let mut a = input(0, area, area);
        a.z_index = 5;
        let mut b = input(1, area, area);
        b.z_index = -1;
        b.focused = true;
        let mut c = input(2, area, area);
        c.z_index = 5;
        let scene = SceneSnapshot::new(area, [a, b, c]).unwrap();
        assert_eq!(
            scene
                .entries()
                .iter()
                .map(SceneEntry::window)
                .collect::<Vec<_>>(),
            [id(1), id(0), id(2)]
        );
        assert!(scene.entries()[0].focused());
        assert_eq!(
            ids(scene.hit_candidates(Point { x: 10., y: 10. })),
            [id(2), id(0), id(1)]
        );
        assert_eq!(
            scene.hit_test(Point { x: 10., y: 10. }).unwrap().window(),
            id(2)
        );
    }

    #[test]
    fn oversized_client_retains_actual_geometry_and_picks_only_allocated_clip() {
        let viewport = rect(0., 0., 200., 150.);
        let visual = rect(-50., -10., 500., 400.);
        let clip = rect(20., 30., 80., 60.);
        let scene = SceneSnapshot::new(viewport, [input(0, visual, clip)]).unwrap();
        let entry = &scene.entries()[0];
        assert_eq!(entry.visual_rect(), visual);
        assert_eq!(entry.container_clip(), clip);
        assert_eq!(entry.effective_clip(), clip);
        assert_eq!(entry.visible_rect(), clip);
        assert!(scene.hit_test(Point { x: 19., y: 40. }).is_none());
        assert!(scene.hit_test(Point { x: 20., y: 30. }).is_some());
        assert!(scene.hit_test(Point { x: 100., y: 40. }).is_none());
        assert!(scene.hit_test(Point { x: 40., y: 90. }).is_none());
    }

    #[test]
    fn hidden_offscreen_and_disjoint_visual_container_have_no_scene_entry() {
        let viewport = rect(0., 0., 100., 100.);
        let mut hidden = input(0, viewport, viewport);
        hidden.visible = false;
        let scene = SceneSnapshot::new(
            viewport,
            [
                hidden,
                input(1, rect(110., 0., 20., 20.), rect(110., 0., 20., 20.)),
                input(2, rect(0., 0., 20., 20.), rect(40., 40., 20., 20.)),
            ],
        )
        .unwrap();
        assert!(scene.is_empty());
        assert_eq!(scene.len(), 0);
        assert_eq!(scene.viewport(), viewport);
        assert!(
            scene
                .clip_candidates(Point { x: 5., y: 5. })
                .next()
                .is_none()
        );
    }

    #[test]
    fn allocation_candidates_are_distinct_from_visual_hits_and_clip_to_viewport() {
        let scene = SceneSnapshot::new(
            rect(0., 0., 100., 100.),
            [input(
                0,
                rect(20., 20., 10., 10.),
                rect(-10., -10., 200., 200.),
            )],
        )
        .unwrap();
        assert_eq!(
            scene.entries()[0].effective_clip(),
            rect(0., 0., 100., 100.)
        );
        assert!(scene.hit_test(Point { x: 50., y: 50. }).is_none());
        assert_eq!(
            ids(scene.clip_candidates(Point { x: 50., y: 50. })),
            [id(0)]
        );
        assert!(
            scene
                .clip_candidates(Point { x: -1., y: 50. })
                .next()
                .is_none()
        );
        assert!(
            scene
                .clip_candidates(Point { x: 100., y: 50. })
                .next()
                .is_none()
        );
    }

    #[test]
    fn negative_origins_and_adjacent_half_open_boundaries_have_one_owner() {
        let viewport = rect(-100., -50., 200., 100.);
        let scene = SceneSnapshot::new(
            viewport,
            [
                input(0, rect(-100., -50., 100., 100.), viewport),
                input(1, rect(0., -50., 100., 100.), viewport),
            ],
        )
        .unwrap();
        for (point, expected) in [
            (Point { x: -100., y: -50. }, Some(id(0))),
            (Point { x: -0.001, y: 0. }, Some(id(0))),
            (Point { x: 0., y: 0. }, Some(id(1))),
            (Point { x: 100., y: 0. }, None),
            (Point { x: 0., y: 50. }, None),
        ] {
            assert_eq!(scene.hit_test(point).map(SceneEntry::window), expected);
            assert!(scene.hit_candidates(point).count() <= 1);
        }
        for bad in [
            Point { x: f64::NAN, y: 0. },
            Point {
                x: 0.,
                y: f64::INFINITY,
            },
            Point {
                x: f64::NEG_INFINITY,
                y: 0.,
            },
        ] {
            assert!(scene.hit_test(bad).is_none());
            assert!(scene.clip_candidates(bad).next().is_none());
        }
    }

    #[test]
    fn invalid_hidden_geometry_nonpositive_sizes_overflow_and_loss_of_extent_are_rejected() {
        let viewport = rect(0., 0., 100., 100.);
        for invalid in [
            rect(f64::NAN, 0., 1., 1.),
            rect(0., 0., f64::INFINITY, 1.),
            rect(0., 0., 0., 1.),
            rect(0., 0., -1., 1.),
            rect(f64::MAX, 0., f64::MAX, 1.),
            rect(1e100, 0., 1., 1.),
            rect(1e100, 0., 1e99, 1.),
        ] {
            let mut bad = input(0, invalid, viewport);
            bad.visible = false;
            assert!(matches!(
                SceneSnapshot::new(viewport, [bad]),
                Err(SceneError::InvalidEntry {
                    field: GeometryField::VisualRect,
                    ..
                })
            ));
            assert!(SceneSnapshot::new(invalid, []).is_err());
            assert!(matches!(
                SceneSnapshot::new(viewport, [input(0, viewport, invalid)]),
                Err(SceneError::InvalidEntry {
                    field: GeometryField::ContainerClip,
                    ..
                })
            ));
        }
    }

    #[test]
    fn endpoint_limit_is_checked_separately_from_origin_and_width() {
        let viewport = rect(0., 0., 100., 100.);
        assert!(matches!(
            SceneSnapshot::new(viewport, [input(0, rect(MAX_COORD, 0., 1., 1.), viewport)]),
            Err(SceneError::InvalidEntry {
                source: GeometryError::CoordinateLimit,
                ..
            })
        ));
        let full = rect(-MAX_COORD, -MAX_COORD, MAX_COORD * 2., MAX_COORD * 2.);
        let scene = SceneSnapshot::new(full, [input(0, full, full)]).unwrap();
        assert_eq!(
            round_rect_to_i32(scene.entries()[0].visible_rect()).unwrap(),
            Some(IntegerRect {
                x: -1_000_000_000,
                y: -1_000_000_000,
                width: 2_000_000_000,
                height: 2_000_000_000
            })
        );
    }

    #[test]
    fn duplicate_ids_include_hidden_inputs_but_generations_remain_distinct() {
        let area = rect(0., 0., 100., 100.);
        let mut hidden = input(0, area, area);
        hidden.visible = false;
        assert_eq!(
            SceneSnapshot::new(area, [input(0, area, area), hidden]),
            Err(SceneError::DuplicateWindow(id(0)))
        );
        let mut next_generation = input(0, area, area);
        next_generation.window = WindowId::new(0, 1);
        assert_eq!(
            SceneSnapshot::new(area, [input(0, area, area), next_generation])
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn capacity_counts_filtered_inputs_and_consumes_at_most_one_over_limit() {
        use std::cell::Cell;
        let count = Cell::new(0);
        let viewport = rect(0., 0., 100., 100.);
        let inputs = (0u32..).map(|n| {
            count.set(count.get() + 1);
            let mut entry = input(n, viewport, viewport);
            entry.visible = false;
            entry
        });
        assert_eq!(
            SceneSnapshot::new(viewport, inputs),
            Err(SceneError::CapacityExceeded)
        );
        assert_eq!(count.get(), MAX_ENTRIES + 1);
        let inputs = (0..MAX_ENTRIES as u32).map(|n| input(n, viewport, viewport));
        assert_eq!(
            SceneSnapshot::new(viewport, inputs).unwrap().len(),
            MAX_ENTRIES
        );
    }

    #[test]
    fn snapshot_owns_copied_geometry_and_clone_preserves_pick_order() {
        let area = rect(0., 0., 100., 100.);
        let mut source = [input(0, area, area)];
        let scene = SceneSnapshot::new(area, source.iter().copied()).unwrap();
        source[0].visual_rect = rect(500., 500., 1., 1.);
        source[0].focused = true;
        assert!(source[0].focused);
        assert_ne!(source[0].visual_rect, scene.entries()[0].visual_rect());
        assert_eq!(scene.entries()[0].visual_rect(), area);
        assert!(!scene.entries()[0].focused());
        assert_eq!(scene.clone(), scene);
    }

    #[test]
    fn intersections_require_positive_area_and_validate_all_fields() {
        assert_eq!(
            intersection(rect(-10., -5., 20., 10.), rect(0., 0., 20., 10.)).unwrap(),
            Some(rect(0., 0., 10., 5.))
        );
        assert_eq!(
            intersection(rect(0., 0., 10., 10.), rect(10., 0., 10., 10.)).unwrap(),
            None
        );
        assert_eq!(
            intersection(rect(0., 0., 10., 10.), rect(0., 10., 10., 10.)).unwrap(),
            None
        );
        assert_eq!(
            intersection(rect(0., 0., 0., 10.), rect(0., 0., 10., 10.)),
            Err(GeometryError::InvalidRect)
        );
    }

    #[test]
    fn integer_rounding_uses_shared_edges_and_rejects_overflow_without_saturation() {
        let left = round_rect_to_i32(rect(-1.4, -0.5, 2., 1.))
            .unwrap()
            .unwrap();
        let right = round_rect_to_i32(rect(0.6, -0.5, 1.6, 1.))
            .unwrap()
            .unwrap();
        assert_eq!(left.x + left.width, right.x);
        assert_eq!((left.y, left.height), (-1, 2));
        assert_eq!(round_rect_to_i32(rect(0.1, 0.1, 0.1, 0.1)).unwrap(), None);
        assert_eq!(
            round_rect_to_i32(rect(1e100, 0., 1e99, 1.)),
            Err(GeometryError::IntegerOverflow)
        );
        assert_eq!(
            round_rect_to_i32(rect(f64::from(i32::MAX), 0., 1., 1.)),
            Err(GeometryError::IntegerOverflow)
        );
        assert_eq!(
            round_rect_to_i32(rect(f64::from(i32::MIN), 0., f64::from(u32::MAX), 1.)),
            Err(GeometryError::IntegerOverflow)
        );
        assert_eq!(
            round_rect_to_i32(rect(0., 0., f64::NAN, 1.)),
            Err(GeometryError::InvalidRect)
        );
    }
}
