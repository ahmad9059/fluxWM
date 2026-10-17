//! Pure, deterministic workspace layout. No protocol, renderer, or input-device ownership.
//!
//! Positions are logical world coordinates. `Placement::rect` must be transformed through
//! [`Camera`] before presentation. Hidden tabs keep nonzero restoration geometry.
use flux_core::{Direction, LayoutMode, Point, Rect, Size, WindowId};
use std::collections::{BTreeMap, BTreeSet};

/// Conservative coordinate limit keeps additions and transforms far from overflow.
pub const MAX_COORD: f64 = 1_000_000_000.0;
const MIN_SIZE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum LayoutError {
    #[error("geometry must be finite, positive, and within supported coordinate limits")]
    InvalidGeometry,
    #[error("window is already a member")]
    DuplicateWindow,
    #[error("window is not a member")]
    UnknownWindow,
    #[error("window size constraints conflict")]
    InvalidConstraints,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeConstraints {
    pub min: Size,
    pub max: Option<Size>,
}
impl Default for SizeConstraints {
    fn default() -> Self {
        Self {
            min: Size {
                width: 1.0,
                height: 1.0,
            },
            max: None,
        }
    }
}
impl SizeConstraints {
    fn valid(self) -> bool {
        valid_size(self.min)
            && self.max.is_none_or(|m| {
                valid_size(m) && m.width >= self.min.width && m.height >= self.min.height
            })
    }
    fn size(self, size: Size) -> Size {
        Size {
            width: size
                .width
                .max(self.min.width)
                .min(self.max.map_or(MAX_COORD, |m| m.width)),
            height: size
                .height
                .max(self.min.height)
                .min(self.max.map_or(MAX_COORD, |m| m.height)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnMode {
    Stack,
    Tabs,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidthPolicy {
    Fixed(f64),
    ViewportFraction(f64),
}
impl WidthPolicy {
    fn resolve(self, viewport: f64) -> Result<f64, LayoutError> {
        let width = match self {
            Self::Fixed(w) if positive(w) => w,
            Self::ViewportFraction(f) if f.is_finite() && f > 0.0 && f <= MAX_COORD => {
                (viewport * f).max(MIN_SIZE)
            }
            _ => return Err(LayoutError::InvalidGeometry),
        };
        if positive(width) {
            Ok(width)
        } else {
            Err(LayoutError::InvalidGeometry)
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: Point,
    pub zoom: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Point::default(),
            zoom: 1.0,
        }
    }
}
impl Camera {
    pub fn new(position: Point, zoom: f64) -> Result<Self, LayoutError> {
        if valid_point(position) && zoom.is_finite() && (0.01..=100.0).contains(&zoom) {
            Ok(Self { position, zoom })
        } else {
            Err(LayoutError::InvalidGeometry)
        }
    }
    pub fn world_to_view(self, p: Point) -> Point {
        Point {
            x: (p.x - self.position.x) * self.zoom,
            y: (p.y - self.position.y) * self.zoom,
        }
    }
    pub fn view_to_world(self, p: Point) -> Point {
        Point {
            x: p.x / self.zoom + self.position.x,
            y: p.y / self.zoom + self.position.y,
        }
    }
    pub fn rect_to_view(self, r: Rect) -> Rect {
        let p = self.world_to_view(Point { x: r.x, y: r.y });
        Rect {
            x: p.x,
            y: p.y,
            width: r.width * self.zoom,
            height: r.height * self.zoom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub window: WindowId,
    pub rect: Rect,
    pub visible: bool,
    pub focused: bool,
}
#[derive(Debug, Clone)]
struct Column {
    windows: Vec<WindowId>,
    width: WidthPolicy,
    mode: ColumnMode,
    active: WindowId,
    scroll_y: f64,
}

/// One workspace. The caller handles output ownership and maps its usable region to `viewport`.
#[derive(Debug, Clone)]
pub struct Layout {
    viewport: Size,
    gap: f64,
    mode: LayoutMode,
    columns: Vec<Column>,
    members: BTreeMap<WindowId, SizeConstraints>,
    canvas: BTreeMap<WindowId, Rect>,
    focus: Option<WindowId>,
    history: Vec<WindowId>,
    strip_camera: Camera,
    canvas_camera: Camera,
}
impl Layout {
    pub fn new(viewport: Size) -> Result<Self, LayoutError> {
        if !valid_size(viewport) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            viewport,
            gap: 8.0,
            mode: LayoutMode::Strip,
            columns: Vec::new(),
            members: BTreeMap::new(),
            canvas: BTreeMap::new(),
            focus: None,
            history: Vec::new(),
            strip_camera: Camera::default(),
            canvas_camera: Camera::default(),
        })
    }
    pub fn mode(&self) -> LayoutMode {
        self.mode
    }
    pub fn focused(&self) -> Option<WindowId> {
        self.focus
    }
    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    pub fn viewport(&self) -> Size {
        self.viewport
    }
    pub fn camera(&self) -> Camera {
        match self.mode {
            LayoutMode::Strip => self.strip_camera,
            LayoutMode::Canvas => self.canvas_camera,
        }
    }
    fn camera_mut(&mut self) -> &mut Camera {
        match self.mode {
            LayoutMode::Strip => &mut self.strip_camera,
            LayoutMode::Canvas => &mut self.canvas_camera,
        }
    }
    pub fn set_camera(&mut self, camera: Camera) -> Result<(), LayoutError> {
        let camera = Camera::new(camera.position, camera.zoom)?;
        if self.mode == LayoutMode::Strip && camera.zoom != 1.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        *self.camera_mut() = camera;
        Ok(())
    }
    /// Empty usable areas are rejected without changing the previous valid geometry.
    pub fn set_viewport(&mut self, viewport: Size) -> Result<(), LayoutError> {
        if !valid_size(viewport) {
            return Err(LayoutError::InvalidGeometry);
        }
        self.check_strip_extent(viewport.width, self.gap, None, None, 0.0)?;
        self.viewport = viewport;
        self.reveal_focus();
        Ok(())
    }
    pub fn set_gap(&mut self, gap: f64) -> Result<(), LayoutError> {
        if !gap.is_finite() || !(0.0..=1024.0).contains(&gap) {
            return Err(LayoutError::InvalidGeometry);
        }
        self.check_strip_extent(self.viewport.width, gap, None, None, 0.0)?;
        self.gap = gap;
        self.reveal_focus();
        Ok(())
    }
    pub fn insert(
        &mut self,
        window: WindowId,
        constraints: SizeConstraints,
    ) -> Result<(), LayoutError> {
        self.insert_with_focus(window, constraints, true)
    }
    /// Insert according to the compositor's activation decision. An empty workspace
    /// selects its first member even when `activate` is false so focus stays valid.
    pub fn insert_with_focus(
        &mut self,
        window: WindowId,
        constraints: SizeConstraints,
        activate: bool,
    ) -> Result<(), LayoutError> {
        if self.members.contains_key(&window) {
            return Err(LayoutError::DuplicateWindow);
        }
        if !constraints.valid() {
            return Err(LayoutError::InvalidConstraints);
        }
        self.check_strip_extent(
            self.viewport.width,
            self.gap,
            None,
            None,
            (self.viewport.width * 0.5).max(constraints.min.width) + self.gap,
        )?;
        let index = self
            .focus
            .and_then(|w| self.column_index(w))
            .map_or(self.columns.len(), |i| i + 1);
        self.members.insert(window, constraints);
        self.columns.insert(
            index,
            Column {
                windows: vec![window],
                width: WidthPolicy::ViewportFraction(0.5),
                mode: ColumnMode::Stack,
                active: window,
                scroll_y: 0.0,
            },
        );
        // Seed only the new canvas member. Existing canvas positions survive strip operations.
        let strip = self.strip_placements();
        let mut rect = strip.iter().find(|p| p.window == window).unwrap().rect;
        if self.mode == LayoutMode::Canvas
            && let Some(r) = self.focus.and_then(|w| self.canvas.get(&w))
        {
            rect.x = (r.x + r.width + self.gap).min(MAX_COORD - rect.width);
            rect.y = r.y;
        }
        self.canvas.insert(window, rect);
        if activate || self.focus.is_none() {
            self.focus_window(window)
        } else {
            Ok(())
        }
    }
    pub fn remove(&mut self, window: WindowId) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        let anchor = if self.mode == LayoutMode::Strip && self.focus != Some(window) {
            self.focus.map(|w| {
                (
                    w,
                    self.strip_placements()
                        .into_iter()
                        .find(|p| p.window == w)
                        .unwrap()
                        .rect
                        .x,
                )
            })
        } else {
            None
        };
        let c = &mut self.columns[index];
        c.windows.retain(|w| *w != window);
        if c.windows.is_empty() {
            self.columns.remove(index);
        } else if c.active == window {
            c.active = c.windows[0];
        }
        self.members.remove(&window);
        self.canvas.remove(&window);
        self.history.retain(|w| *w != window);
        if self.focus == Some(window) {
            self.focus = None;
            if let Some(next) = self.history.last().copied().or_else(|| {
                self.columns
                    .get(index.min(self.columns.len().saturating_sub(1)))
                    .map(|c| c.active)
            }) {
                self.focus_window(next)?;
            }
        }
        if let Some((w, old_x)) = anchor {
            let new_x = self
                .strip_placements()
                .into_iter()
                .find(|p| p.window == w)
                .unwrap()
                .rect
                .x;
            self.strip_camera.position.x =
                (self.strip_camera.position.x + new_x - old_x).clamp(-MAX_COORD, MAX_COORD);
            self.reveal_focus();
        }
        if self.is_empty() {
            self.strip_camera = Camera::default();
            self.canvas_camera = Camera::default();
        }
        Ok(())
    }
    pub fn focus_window(&mut self, window: WindowId) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        self.columns[index].active = window;
        self.focus = Some(window);
        self.history.retain(|w| *w != window);
        self.history.push(window);
        self.reveal_focus();
        Ok(())
    }
    pub fn set_constraints(
        &mut self,
        window: WindowId,
        constraints: SizeConstraints,
    ) -> Result<(), LayoutError> {
        if !constraints.valid() {
            return Err(LayoutError::InvalidConstraints);
        }
        if !self.members.contains_key(&window) {
            return Err(LayoutError::UnknownWindow);
        }
        self.check_strip_extent(
            self.viewport.width,
            self.gap,
            Some((window, constraints)),
            None,
            0.0,
        )?;
        let value = self.members.get_mut(&window).unwrap();
        *value = constraints;
        let rect = self.canvas.get_mut(&window).unwrap();
        let size = constraints.size(Size {
            width: rect.width,
            height: rect.height,
        });
        rect.width = size.width;
        rect.height = size.height;
        self.reveal_focus();
        Ok(())
    }
    pub fn set_mode(&mut self, mode: LayoutMode) {
        self.mode = mode;
        self.reveal_focus();
    }
    pub fn set_canvas_rect(&mut self, window: WindowId, rect: Rect) -> Result<(), LayoutError> {
        if !valid_rect(rect) {
            return Err(LayoutError::InvalidGeometry);
        }
        let constraints = *self
            .members
            .get(&window)
            .ok_or(LayoutError::UnknownWindow)?;
        let size = constraints.size(Size {
            width: rect.width,
            height: rect.height,
        });
        self.canvas.insert(
            window,
            Rect {
                width: size.width,
                height: size.height,
                ..rect
            },
        );
        Ok(())
    }
    pub fn pan(&mut self, dx: f64, dy: f64) -> Result<(), LayoutError> {
        let old = self.camera();
        self.set_camera(Camera {
            position: Point {
                x: old.position.x + dx,
                y: old.position.y + dy,
            },
            ..old
        })
    }
    pub fn set_column_width(
        &mut self,
        window: WindowId,
        policy: WidthPolicy,
    ) -> Result<(), LayoutError> {
        policy.resolve(self.viewport.width)?;
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        self.check_strip_extent(
            self.viewport.width,
            self.gap,
            None,
            Some((index, policy)),
            0.0,
        )?;
        self.columns[index].width = policy;
        self.reveal_focus();
        Ok(())
    }
    pub fn set_column_mode(
        &mut self,
        window: WindowId,
        mode: ColumnMode,
    ) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        self.columns[index].mode = mode;
        self.reveal_focus();
        Ok(())
    }
    /// Move a member into another column, with no duplicate or orphan intermediate state.
    pub fn join_column(&mut self, window: WindowId, target: WindowId) -> Result<(), LayoutError> {
        let source = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        let dest = self
            .column_index(target)
            .ok_or(LayoutError::UnknownWindow)?;
        if source == dest {
            return Ok(());
        }
        self.columns[source].windows.retain(|w| *w != window);
        if self.columns[source].active == window {
            self.columns[source].active = self.columns[source]
                .windows
                .first()
                .copied()
                .unwrap_or(window);
        }
        self.columns[dest].windows.push(window);
        self.columns[dest].active = window;
        if self.columns[source].windows.is_empty() {
            self.columns.remove(source);
        }
        self.focus_window(window)
    }
    pub fn resize_focused(&mut self, delta: f64) -> Result<(), LayoutError> {
        if !delta.is_finite() {
            return Err(LayoutError::InvalidGeometry);
        }
        let Some(window) = self.focus else {
            return Ok(());
        };
        if self.mode == LayoutMode::Canvas {
            let r = self.canvas[&window];
            self.set_canvas_rect(
                window,
                Rect {
                    width: r.width + delta,
                    ..r
                },
            )?;
        } else {
            let i = self.column_index(window).unwrap();
            let width = self.columns[i].width.resolve(self.viewport.width)? + delta;
            self.set_column_width(window, WidthPolicy::Fixed(width))?;
        }
        self.reveal_focus();
        Ok(())
    }
    pub fn move_focused(&mut self, direction: Direction) -> Result<(), LayoutError> {
        let Some(window) = self.focus else {
            return Ok(());
        };
        if self.mode == LayoutMode::Canvas {
            let mut r = self.canvas[&window];
            match direction {
                Direction::Left => r.x -= 32.0,
                Direction::Right => r.x += 32.0,
                Direction::Up => r.y -= 32.0,
                Direction::Down => r.y += 32.0,
            }
            self.set_canvas_rect(window, r)?;
        } else {
            let old_x = self
                .strip_placements()
                .into_iter()
                .find(|p| p.window == window)
                .unwrap()
                .rect
                .x;
            let i = self.column_index(window).unwrap();
            match direction {
                Direction::Left if i > 0 => self.columns.swap(i, i - 1),
                Direction::Right if i + 1 < self.columns.len() => self.columns.swap(i, i + 1),
                Direction::Up | Direction::Down => {
                    let c = &mut self.columns[i];
                    let j = c.windows.iter().position(|w| *w == window).unwrap();
                    if direction == Direction::Up && j > 0 {
                        c.windows.swap(j, j - 1);
                    } else if direction == Direction::Down && j + 1 < c.windows.len() {
                        c.windows.swap(j, j + 1);
                    }
                }
                _ => {}
            }
            let new_x = self
                .strip_placements()
                .into_iter()
                .find(|p| p.window == window)
                .unwrap()
                .rect
                .x;
            self.strip_camera.position.x =
                (self.strip_camera.position.x + new_x - old_x).clamp(-MAX_COORD, MAX_COORD);
        }
        self.reveal_focus();
        Ok(())
    }
    pub fn focus_direction(
        &mut self,
        direction: Direction,
    ) -> Result<Option<WindowId>, LayoutError> {
        let Some(window) = self.focus else {
            return Ok(None);
        };
        let next = if self.mode == LayoutMode::Strip {
            let i = self.column_index(window).unwrap();
            let c = &self.columns[i];
            let j = c.windows.iter().position(|w| *w == window).unwrap();
            match direction {
                Direction::Left => i.checked_sub(1).map(|i| self.columns[i].active),
                Direction::Right => self.columns.get(i + 1).map(|c| c.active),
                Direction::Up => j.checked_sub(1).map(|j| c.windows[j]),
                Direction::Down => c.windows.get(j + 1).copied(),
            }
        } else {
            let origin = center(self.canvas[&window]);
            self.canvas
                .iter()
                .filter(|(id, _)| **id != window)
                .filter_map(|(id, r)| {
                    let p = center(*r);
                    let (forward, perpendicular) = match direction {
                        Direction::Left => (origin.x - p.x, (origin.y - p.y).abs()),
                        Direction::Right => (p.x - origin.x, (origin.y - p.y).abs()),
                        Direction::Up => (origin.y - p.y, (origin.x - p.x).abs()),
                        Direction::Down => (p.y - origin.y, (origin.x - p.x).abs()),
                    };
                    (forward > 0.0).then_some((
                        *id,
                        perpendicular / forward,
                        forward,
                        perpendicular,
                    ))
                })
                .min_by(|a, b| {
                    a.1.total_cmp(&b.1)
                        .then(a.2.total_cmp(&b.2))
                        .then(a.3.total_cmp(&b.3))
                        .then(a.0.cmp(&b.0))
                })
                .map(|v| v.0)
        };
        if let Some(next) = next {
            self.focus_window(next)?;
        }
        Ok(next)
    }
    /// All members, including hidden tabs and offscreen content, in deterministic scene order.
    pub fn placements(&self) -> Vec<Placement> {
        if self.mode == LayoutMode::Strip {
            self.strip_placements()
        } else {
            self.canvas
                .iter()
                .map(|(window, rect)| Placement {
                    window: *window,
                    rect: *rect,
                    visible: true,
                    focused: self.focus == Some(*window),
                })
                .collect()
        }
    }
    pub fn visible_placements(&self) -> Vec<Placement> {
        let camera = self.camera();
        self.placements()
            .into_iter()
            .filter(|p| {
                let r = camera.rect_to_view(p.rect);
                p.visible
                    && r.x < self.viewport.width
                    && r.y < self.viewport.height
                    && r.x + r.width > 0.0
                    && r.y + r.height > 0.0
            })
            .collect()
    }
    /// Pick in output-local logical coordinates; topmost focused canvas member wins overlaps.
    pub fn hit_test(&self, point: Point) -> Option<WindowId> {
        if !valid_point(point)
            || point.x < 0.0
            || point.y < 0.0
            || point.x >= self.viewport.width
            || point.y >= self.viewport.height
        {
            return None;
        }
        let world = self.camera().view_to_world(point);
        let placements = self.placements();
        placements
            .iter()
            .find(|p| p.focused && p.visible && p.rect.contains(world))
            .or_else(|| {
                placements
                    .iter()
                    .rev()
                    .find(|p| p.visible && p.rect.contains(world))
            })
            .map(|p| p.window)
    }
    /// Shift every canvas location and its camera atomically. Returned delta must also be
    /// applied by the caller to any active gesture, animation, or external spatial index.
    /// Strip coordinates are prefix sums and are intentionally unaffected.
    pub fn rebase_canvas(&mut self, delta: Point) -> Result<(), LayoutError> {
        if !valid_point(delta) {
            return Err(LayoutError::InvalidGeometry);
        }
        let shift = |p: Point| Point {
            x: p.x - delta.x,
            y: p.y - delta.y,
        };
        let camera = shift(self.canvas_camera.position);
        if !valid_point(camera)
            || self
                .canvas
                .values()
                .any(|r| !valid_point(shift(Point { x: r.x, y: r.y })))
        {
            return Err(LayoutError::InvalidGeometry);
        }
        for r in self.canvas.values_mut() {
            r.x -= delta.x;
            r.y -= delta.y;
        }
        self.canvas_camera.position = camera;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let flattened: Vec<_> = self
            .columns
            .iter()
            .flat_map(|c| c.windows.iter().copied())
            .collect();
        let unique: BTreeSet<_> = flattened.iter().copied().collect();
        if unique.len() != flattened.len()
            || unique.len() != self.members.len()
            || unique
                .iter()
                .any(|w| !self.members.contains_key(w) || !self.canvas.contains_key(w))
            || self.canvas.len() != unique.len()
        {
            return Err("membership mismatch");
        }
        if self
            .columns
            .iter()
            .any(|c| c.windows.is_empty() || !c.windows.contains(&c.active))
        {
            return Err("invalid column");
        }
        if self.focus.is_some_and(|w| !unique.contains(&w))
            || (!unique.is_empty() && self.focus.is_none())
        {
            return Err("invalid focus");
        }
        if self.history.iter().any(|w| !unique.contains(w)) {
            return Err("stale focus history");
        }
        if self
            .placements()
            .iter()
            .any(|p| !p.rect.is_valid() || p.rect.width < MIN_SIZE || p.rect.height < MIN_SIZE)
        {
            return Err("invalid placement");
        }
        for c in [self.strip_camera, self.canvas_camera] {
            if Camera::new(c.position, c.zoom).is_err() {
                return Err("invalid camera");
            }
        }
        Ok(())
    }
    fn check_strip_extent(
        &self,
        viewport: f64,
        gap: f64,
        constraints: Option<(WindowId, SizeConstraints)>,
        policy: Option<(usize, WidthPolicy)>,
        extra: f64,
    ) -> Result<(), LayoutError> {
        let mut extent = extra;
        for (i, c) in self.columns.iter().enumerate() {
            let width = policy
                .filter(|(index, _)| *index == i)
                .map_or(c.width, |(_, p)| p)
                .resolve(viewport)?;
            let minimum = c
                .windows
                .iter()
                .map(|w| {
                    constraints
                        .filter(|(id, _)| id == w)
                        .map_or(self.members[w], |(_, value)| value)
                        .min
                        .width
                })
                .fold(width, f64::max);
            extent += minimum + gap;
            if extent > MAX_COORD {
                return Err(LayoutError::InvalidGeometry);
            }
        }
        if extent > MAX_COORD {
            Err(LayoutError::InvalidGeometry)
        } else {
            Ok(())
        }
    }
    fn column_index(&self, window: WindowId) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.windows.contains(&window))
    }
    fn strip_placements(&self) -> Vec<Placement> {
        let mut x = 0.0;
        let mut result = Vec::with_capacity(self.len());
        for c in &self.columns {
            let requested = c
                .width
                .resolve(self.viewport.width)
                .expect("validated width policy");
            let width = c
                .windows
                .iter()
                .map(|w| self.members[w].min.width)
                .fold(requested, f64::max);
            let gaps = self.gap * c.windows.len().saturating_sub(1) as f64;
            let available = (self.viewport.height - gaps).max(0.0);
            let heights = allocate_heights(&c.windows, &self.members, available);
            let mut y = -c.scroll_y;
            for (i, window) in c.windows.iter().enumerate() {
                let size = self.members[window].size(Size {
                    width,
                    height: if c.mode == ColumnMode::Tabs {
                        self.viewport.height
                    } else {
                        heights[i]
                    },
                });
                result.push(Placement {
                    window: *window,
                    rect: Rect {
                        x,
                        y: if c.mode == ColumnMode::Tabs { 0.0 } else { y },
                        width: size.width,
                        height: size.height,
                    },
                    visible: c.mode == ColumnMode::Stack || *window == c.active,
                    focused: self.focus == Some(*window),
                });
                y += size.height + self.gap;
            }
            x += width + self.gap;
        }
        result
    }
    fn reveal_focus(&mut self) {
        let Some(window) = self.focus else {
            return;
        };
        if self.mode == LayoutMode::Strip {
            let i = self.column_index(window).unwrap();
            if self.columns[i].mode == ColumnMode::Stack {
                let rect = self
                    .strip_placements()
                    .into_iter()
                    .find(|p| p.window == window)
                    .unwrap()
                    .rect;
                let shift = reveal_axis(0.0, self.viewport.height, rect.y, rect.height);
                self.columns[i].scroll_y = (self.columns[i].scroll_y + shift).max(0.0);
            }
        }
        let rect = self
            .placements()
            .into_iter()
            .find(|p| p.window == window)
            .unwrap()
            .rect;
        let camera = self.camera();
        let x = reveal_axis(
            camera.position.x,
            self.viewport.width / camera.zoom,
            rect.x,
            rect.width,
        );
        let y = if self.mode == LayoutMode::Strip {
            0.0
        } else {
            reveal_axis(
                camera.position.y,
                self.viewport.height / camera.zoom,
                rect.y,
                rect.height,
            )
        };
        self.camera_mut().position = Point {
            x: x.clamp(-MAX_COORD, MAX_COORD),
            y: y.clamp(-MAX_COORD, MAX_COORD),
        };
    }
}
fn allocate_heights(
    windows: &[WindowId],
    members: &BTreeMap<WindowId, SizeConstraints>,
    available: f64,
) -> Vec<f64> {
    let mut heights: Vec<_> = windows.iter().map(|w| members[w].min.height).collect();
    let mut remaining = (available - heights.iter().sum::<f64>()).max(0.0);
    let mut eligible: Vec<_> = (0..windows.len()).collect();
    while remaining > 0.000001 && !eligible.is_empty() {
        let share = remaining / eligible.len() as f64;
        let mut used = 0.0;
        eligible.retain(|i| {
            let max = members[&windows[*i]].max.map_or(MAX_COORD, |m| m.height);
            let growth = share.min((max - heights[*i]).max(0.0));
            heights[*i] += growth;
            used += growth;
            heights[*i] + 0.000001 < max
        });
        remaining -= used;
        if used == 0.0 {
            break;
        }
    }
    heights
}
fn reveal_axis(camera: f64, viewport: f64, start: f64, size: f64) -> f64 {
    if size >= viewport || start < camera {
        start
    } else if start + size > camera + viewport {
        start + size - viewport
    } else {
        camera
    }
}
fn positive(v: f64) -> bool {
    v.is_finite() && (MIN_SIZE..=MAX_COORD).contains(&v)
}
fn valid_size(s: Size) -> bool {
    positive(s.width) && positive(s.height)
}
fn valid_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD
}
fn valid_rect(r: Rect) -> bool {
    valid_point(Point { x: r.x, y: r.y })
        && valid_size(Size {
            width: r.width,
            height: r.height,
        })
}
fn center(r: Rect) -> Point {
    Point {
        x: r.x + r.width / 2.0,
        y: r.y + r.height / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u32) -> WindowId {
        WindowId::new(n, 1)
    }
    fn layout() -> Layout {
        Layout::new(Size {
            width: 1000.0,
            height: 600.0,
        })
        .unwrap()
    }
    fn rect(l: &Layout, w: WindowId) -> Rect {
        l.placements()
            .into_iter()
            .find(|p| p.window == w)
            .unwrap()
            .rect
    }
    #[test]
    fn independent_columns_scroll_without_shrinking() {
        let mut l = layout();
        for n in 0..20 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        assert_eq!(l.len(), 20);
        for p in l.placements() {
            assert_eq!(p.rect.width, 500.0);
        }
        assert!(l.camera().position.x > 8000.0);
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.camera().position.x, 0.0);
        assert_eq!(l.hit_test(Point { x: 0.0, y: 0.0 }), Some(id(0)));
        assert_eq!(l.hit_test(Point { x: 500.0, y: 1.0 }), None);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn stack_minimum_overflow_is_reachable_and_tabs_preserve_geometry() {
        let mut l = layout();
        let constraints = SizeConstraints {
            min: Size {
                width: 700.0,
                height: 450.0,
            },
            max: None,
        };
        for n in 0..3 {
            l.insert(id(n), constraints).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.join_column(id(2), id(0)).unwrap();
        let r = rect(&l, id(2));
        assert_eq!(r.height, 450.0);
        assert!(r.y >= 0.0 && r.y + r.height <= 600.0);
        l.focus_window(id(0)).unwrap();
        assert_eq!(rect(&l, id(0)).y, 0.0);
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        assert_eq!(l.placements().iter().filter(|p| p.visible).count(), 1);
        l.focus_direction(Direction::Down).unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        assert!(l.placements().iter().all(|p| p.rect.height >= 450.0));
        l.remove(id(1)).unwrap();
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn height_water_filling_respects_maximum_and_distributes_remaining() {
        let mut l = layout();
        l.set_gap(0.0).unwrap();
        l.insert(
            id(0),
            SizeConstraints {
                min: Size {
                    width: 1.0,
                    height: 10.0,
                },
                max: Some(Size {
                    width: 200.0,
                    height: 100.0,
                }),
            },
        )
        .unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        l.join_column(id(1), id(0)).unwrap();
        assert_eq!(rect(&l, id(0)).height, 100.0);
        assert_eq!(rect(&l, id(1)).height, 500.0);
        assert_eq!(rect(&l, id(0)).width, 200.0);
    }
    #[test]
    fn canvas_has_two_axes_negative_coordinates_and_restores_modes() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        let saved = l.placements();
        l.set_mode(LayoutMode::Canvas);
        let far = Rect {
            x: -100_000.0,
            y: 200_000.0,
            width: 400.0,
            height: 300.0,
        };
        l.set_canvas_rect(id(0), far).unwrap();
        l.focus_window(id(0)).unwrap();
        assert!(l.camera().position.x < 0.0);
        assert!(l.camera().position.y > 0.0);
        let view = l.camera().rect_to_view(far);
        assert_eq!(
            l.hit_test(Point {
                x: view.x + 1.0,
                y: view.y + 1.0
            }),
            Some(id(0))
        );
        l.set_mode(LayoutMode::Strip);
        assert_eq!(
            l.placements().iter().map(|p| p.rect).collect::<Vec<_>>(),
            saved.iter().map(|p| p.rect).collect::<Vec<_>>()
        );
        l.set_mode(LayoutMode::Canvas);
        assert_eq!(rect(&l, id(0)), far);
    }
    #[test]
    fn transform_round_trip_and_rebase_preserve_screen_geometry() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.set_mode(LayoutMode::Canvas);
        l.set_canvas_rect(
            id(0),
            Rect {
                x: 99_000_000.0,
                y: -99_000_000.0,
                width: 500.0,
                height: 600.0,
            },
        )
        .unwrap();
        l.set_camera(
            Camera::new(
                Point {
                    x: 99_000_001.0,
                    y: -98_999_900.0,
                },
                0.25,
            )
            .unwrap(),
        )
        .unwrap();
        let before = l.camera().rect_to_view(rect(&l, id(0)));
        let world = Point {
            x: 98_765_432.25,
            y: -98_012_345.5,
        };
        assert_eq!(
            l.camera().view_to_world(l.camera().world_to_view(world)),
            world
        );
        l.rebase_canvas(Point {
            x: 99_000_000.0,
            y: -99_000_000.0,
        })
        .unwrap();
        assert_eq!(l.camera().rect_to_view(rect(&l, id(0))), before);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn invalid_mutations_preserve_valid_state_and_generations_are_distinct() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.insert(WindowId::new(0, 2), SizeConstraints::default())
            .unwrap();
        let before = l.placements();
        assert_eq!(
            l.insert(id(0), SizeConstraints::default()),
            Err(LayoutError::DuplicateWindow)
        );
        assert_eq!(
            l.set_viewport(Size {
                width: 0.0,
                height: 0.0
            }),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.pan(f64::NAN, 0.0), Err(LayoutError::InvalidGeometry));
        assert_eq!(
            l.resize_focused(f64::INFINITY),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(
            l.set_constraints(
                id(0),
                SizeConstraints {
                    min: Size {
                        width: 2.0,
                        height: 1.0
                    },
                    max: Some(Size {
                        width: 1.0,
                        height: 1.0
                    })
                }
            ),
            Err(LayoutError::InvalidConstraints)
        );
        assert_eq!(l.remove(id(100)), Err(LayoutError::UnknownWindow));
        assert_eq!(before, l.placements());
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn canvas_directional_focus_has_stable_ties_and_ignores_viewport() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        for (n, x, y) in [(0, 0.0, 0.0), (1, 100_000.0, 0.0), (2, 100_000.0, 0.0)] {
            l.set_canvas_rect(
                id(n),
                Rect {
                    x,
                    y,
                    width: 100.0,
                    height: 100.0,
                },
            )
            .unwrap();
        }
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(1)));
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), None);
    }
    #[test]
    fn tiny_viewport_and_extent_capacity_are_handled_without_partial_mutation() {
        let mut l = Layout::new(Size {
            width: 1.0,
            height: 1.0,
        })
        .unwrap();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        assert_eq!(rect(&l, id(0)).width, 1.0);
        l.set_column_width(id(0), WidthPolicy::Fixed(MAX_COORD - 16.0))
            .unwrap();
        let before = l.placements();
        let huge = SizeConstraints {
            min: Size {
                width: MAX_COORD,
                height: 1.0,
            },
            max: None,
        };
        assert_eq!(l.insert(id(1), huge), Err(LayoutError::InvalidGeometry));
        assert_eq!(l.set_gap(1024.0), Err(LayoutError::InvalidGeometry));
        assert_eq!(l.placements(), before);
        assert_eq!(l.len(), 1);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn reorder_preserves_focused_screen_anchor() {
        let mut l = layout();
        for n in 0..4 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        let before = l.camera().rect_to_view(rect(&l, id(3)));
        l.move_focused(Direction::Left).unwrap();
        assert_eq!(l.camera().rect_to_view(rect(&l, id(3))), before);
    }
    #[test]
    fn unactivated_insert_and_unrelated_close_preserve_focus_and_anchor() {
        let mut l = layout();
        for n in 0..4 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        let camera = l.camera();
        l.insert_with_focus(id(4), SizeConstraints::default(), false)
            .unwrap();
        assert_eq!(l.focused(), Some(id(3)));
        assert_eq!(l.camera(), camera);
        let anchor = l.camera().rect_to_view(rect(&l, id(3)));
        l.remove(id(0)).unwrap();
        assert_eq!(l.focused(), Some(id(3)));
        assert_eq!(l.camera().rect_to_view(rect(&l, id(3))), anchor);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn randomized_10000_commands_preserve_membership_focus_and_geometry() {
        let mut l = layout();
        let mut seed = 0x09e3_7715_8463_a910_u64;
        let mut live = BTreeSet::new();
        let mut next = 0;
        for _ in 0..10_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let target = live
                .iter()
                .nth((seed as usize / 13) % live.len().max(1))
                .copied();
            match seed % 12 {
                0 if live.len() < 100 => {
                    let w = id(next);
                    next += 1;
                    l.insert(w, SizeConstraints::default()).unwrap();
                    live.insert(w);
                }
                1 => {
                    if let Some(w) = target {
                        l.remove(w).unwrap();
                        live.remove(&w);
                    }
                }
                2 => {
                    if let Some(w) = target {
                        l.focus_window(w).unwrap();
                    }
                }
                3 => {
                    l.set_mode(if seed & 128 == 0 {
                        LayoutMode::Strip
                    } else {
                        LayoutMode::Canvas
                    });
                }
                4 => {
                    l.pan((seed % 31) as f64 - 15.0, (seed % 37) as f64 - 18.0)
                        .unwrap();
                }
                5 => {
                    let _ = l.resize_focused((seed % 51) as f64 - 25.0);
                }
                6 => {
                    if let Some(w) = target {
                        l.set_column_mode(
                            w,
                            if seed & 128 == 0 {
                                ColumnMode::Stack
                            } else {
                                ColumnMode::Tabs
                            },
                        )
                        .unwrap();
                    }
                }
                7 => {
                    if let (Some(w), Some(f)) = (target, l.focused()) {
                        l.join_column(w, f).unwrap();
                    }
                }
                8 => {
                    l.set_viewport(Size {
                        width: (seed % 1500 + 1) as f64,
                        height: (seed % 900 + 1) as f64,
                    })
                    .unwrap();
                }
                9 => {
                    let d = [
                        Direction::Left,
                        Direction::Right,
                        Direction::Up,
                        Direction::Down,
                    ][(seed / 19 % 4) as usize];
                    l.focus_direction(d).unwrap();
                }
                10 => {
                    let d = [
                        Direction::Left,
                        Direction::Right,
                        Direction::Up,
                        Direction::Down,
                    ][(seed / 23 % 4) as usize];
                    l.move_focused(d).unwrap();
                }
                _ => {}
            }
            assert_eq!(l.validate(), Ok(()));
            assert_eq!(
                l.placements()
                    .into_iter()
                    .map(|p| p.window)
                    .collect::<BTreeSet<_>>(),
                live
            );
        }
    }
}
