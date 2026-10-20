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
    #[error("window is minimized")]
    MinimizedWindow,
    #[error("operation requires a tiled window")]
    FloatingWindow,
    #[error("window size constraints conflict")]
    InvalidConstraints,
    #[error("layout identity capacity exhausted")]
    CapacityExceeded,
    #[error("canvas group is not a live member")]
    UnknownGroup,
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
    /// Adapt XDG per-axis constraints: zero means unconstrained. Negative values
    /// and contradictory finite bounds are rejected rather than silently coerced.
    pub fn from_xdg(min: (i32, i32), max: (i32, i32)) -> Result<Self, LayoutError> {
        if [min.0, min.1, max.0, max.1].into_iter().any(|v| v < 0) {
            return Err(LayoutError::InvalidConstraints);
        }
        let result = Self {
            min: Size {
                width: f64::from(min.0).max(1.0),
                height: f64::from(min.1).max(1.0),
            },
            max: if max == (0, 0) {
                None
            } else {
                Some(Size {
                    width: if max.0 == 0 {
                        MAX_COORD
                    } else {
                        f64::from(max.0).min(MAX_COORD)
                    },
                    height: if max.1 == 0 {
                        MAX_COORD
                    } else {
                        f64::from(max.1).min(MAX_COORD)
                    },
                })
            },
        };
        if result.valid() {
            Ok(result)
        } else {
            Err(LayoutError::InvalidConstraints)
        }
    }
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
    /// World-space container clip. Render/input integration must intersect this
    /// with output and surface input clips; target client sizes remain unchanged.
    pub clip: Rect,
    pub window: WindowId,
    pub rect: Rect,
    pub visible: bool,
    pub focused: bool,
}
/// Workspace-local stable group identity. Never reused during a Layout's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(u64);
impl GroupId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone, Copy)]
struct CanvasGroup {
    rect: Rect,
    scroll_y: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupGeometry {
    pub id: GroupId,
    pub rect: Rect,
    pub active: WindowId,
    pub mode: ColumnMode,
}
#[derive(Debug, Clone)]
struct Column {
    group: GroupId,
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
    default_width: WidthPolicy,
    mode: LayoutMode,
    columns: Vec<Column>,
    members: BTreeMap<WindowId, SizeConstraints>,
    minimized: BTreeSet<WindowId>,
    maximized: BTreeSet<WindowId>,
    fullscreen: BTreeSet<WindowId>,
    floating: BTreeMap<WindowId, Rect>,
    born_floating: BTreeSet<WindowId>,
    canvas: BTreeMap<GroupId, CanvasGroup>,
    next_group: u64,
    canvas_initialized: bool,
    strip_converted: bool,
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
            default_width: WidthPolicy::ViewportFraction(0.5),
            mode: LayoutMode::Strip,
            columns: Vec::new(),
            members: BTreeMap::new(),
            minimized: BTreeSet::new(),
            maximized: BTreeSet::new(),
            fullscreen: BTreeSet::new(),
            floating: BTreeMap::new(),
            born_floating: BTreeSet::new(),
            canvas: BTreeMap::new(),
            next_group: 0,
            canvas_initialized: false,
            strip_converted: false,
            focus: None,
            history: Vec::new(),
            strip_camera: Camera::default(),
            canvas_camera: Camera::default(),
        })
    }
    pub fn mode(&self) -> LayoutMode {
        self.mode
    }
    pub fn gap(&self) -> f64 {
        self.gap
    }
    /// Apply logical runtime configuration atomically, without protocol effects.
    /// Existing width policies and group identities survive. A changed gap reveals
    /// focus in the current mode; mode conversion follows [`Self::set_mode`].
    /// Failure leaves the complete model, including saved mode state, unchanged.
    pub fn reconfigure(&mut self, gap: f64, mode: LayoutMode) -> Result<(), LayoutError> {
        self.reconfigure_with_default_width(gap, mode, self.default_width)
    }
    /// Atomically change defaults for future columns alongside gap/mode. The
    /// selected policy must resolve for the current viewport. Existing members,
    /// explicit sizes and dormant mode geometry retain their width policies.
    pub fn reconfigure_with_default_width(
        &mut self,
        gap: f64,
        mode: LayoutMode,
        default_width: WidthPolicy,
    ) -> Result<(), LayoutError> {
        default_width.resolve(self.viewport.width)?;
        let mut candidate = self.clone();
        candidate.default_width = default_width;
        if gap != candidate.gap {
            candidate.set_gap(gap)?;
        }
        candidate.set_mode(mode);
        if candidate.mode != mode {
            return Err(LayoutError::InvalidGeometry);
        }
        candidate.check_strip_extent(candidate.viewport.width, gap, None, None, 0.0)?;
        candidate
            .validate()
            .map_err(|_| LayoutError::InvalidGeometry)?;
        // Validate dormant geometry too, without performing another conversion or
        // reveal. A successful publication must remain usable in either mode.
        for saved_mode in [LayoutMode::Strip, LayoutMode::Canvas] {
            candidate.mode = saved_mode;
            if candidate
                .base_placements()
                .iter()
                .any(|p| !valid_rect(p.rect) || !valid_rect(p.clip))
            {
                return Err(LayoutError::InvalidGeometry);
            }
        }
        candidate.mode = mode;
        *self = candidate;
        Ok(())
    }
    /// Width policy copied into each newly admitted column. Existing columns
    /// retain their policies across configuration changes.
    pub fn default_width(&self) -> WidthPolicy {
        self.default_width
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
        self.edit_fullscreen(|candidate| candidate.set_camera_inner(camera))
    }
    fn set_camera_inner(&mut self, camera: Camera) -> Result<(), LayoutError> {
        let camera = Camera::new(camera.position, camera.zoom)?;
        if self.mode == LayoutMode::Strip && camera.zoom != 1.0 {
            return Err(LayoutError::InvalidGeometry);
        }
        if !self.maximized.is_empty() && maximized_rect(self.viewport, camera).is_none() {
            return Err(LayoutError::InvalidGeometry);
        }
        *self.camera_mut() = camera;
        self.clear_incompatible_maximized();
        self.clear_incompatible_fullscreen();
        Ok(())
    }
    /// Empty usable areas are rejected without changing the previous valid geometry.
    pub fn set_viewport(&mut self, viewport: Size) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.set_viewport_inner(viewport))
    }
    fn set_viewport_inner(&mut self, viewport: Size) -> Result<(), LayoutError> {
        if !valid_size(viewport) {
            return Err(LayoutError::InvalidGeometry);
        }
        if !self.maximized.is_empty()
            && [self.strip_camera, self.canvas_camera]
                .iter()
                .any(|camera| maximized_rect(viewport, *camera).is_none())
        {
            return Err(LayoutError::InvalidGeometry);
        }
        self.default_width.resolve(viewport.width)?;
        self.check_strip_extent(viewport.width, self.gap, None, None, 0.0)?;
        self.viewport = viewport;
        self.clear_incompatible_maximized();
        self.clear_incompatible_fullscreen();
        self.reveal_focus();
        Ok(())
    }
    pub fn set_gap(&mut self, gap: f64) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.set_gap_inner(gap))
    }
    fn set_gap_inner(&mut self, gap: f64) -> Result<(), LayoutError> {
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
        self.edit_fullscreen(|candidate| {
            candidate.insert_with_focus_inner(window, constraints, activate)
        })
    }
    /// Admit a window directly into the floating layer. Its private restoration
    /// column consumes no strip space or fullscreen page-bank extent until the
    /// first explicit unfloat. No intermediate tiled focus or geometry is exposed.
    pub fn insert_floating_with_focus(
        &mut self,
        window: WindowId,
        constraints: SizeConstraints,
        initial: Option<Rect>,
        activate: bool,
    ) -> Result<(), LayoutError> {
        if self.members.contains_key(&window) {
            return Err(LayoutError::DuplicateWindow);
        }
        if !constraints.valid() {
            return Err(LayoutError::InvalidConstraints);
        }
        let mut candidate = self.clone();
        let group = GroupId(candidate.next_group);
        candidate.next_group = candidate
            .next_group
            .checked_add(1)
            .ok_or(LayoutError::CapacityExceeded)?;
        let index = candidate
            .focus
            .and_then(|w| candidate.column_index(w))
            .map_or(candidate.columns.len(), |i| i + 1);
        candidate.members.insert(window, constraints);
        let camera = candidate.camera();
        let size = constraints.size(Size {
            width: candidate.viewport.width / camera.zoom * 0.6,
            height: candidate.viewport.height / camera.zoom * 0.6,
        });
        let rect = initial.unwrap_or(Rect {
            x: camera.position.x + (candidate.viewport.width / camera.zoom - size.width) / 2.0,
            y: camera.position.y + (candidate.viewport.height / camera.zoom - size.height) / 2.0,
            width: size.width,
            height: size.height,
        });
        candidate.store_floating_rect(window, rect)?;
        candidate.born_floating.insert(window);
        candidate.columns.insert(
            index,
            Column {
                group,
                windows: vec![window],
                width: candidate.default_width,
                mode: ColumnMode::Stack,
                active: window,
                scroll_y: 0.0,
            },
        );
        candidate.canvas.insert(
            group,
            CanvasGroup {
                rect: candidate.floating[&window],
                scroll_y: 0.0,
            },
        );
        candidate.check_fullscreen_geometry()?;
        if activate || candidate.focus.is_none() {
            candidate.focus_window_inner(window)?;
        }
        *self = candidate;
        Ok(())
    }
    fn dormant_column(&self, column: &Column) -> bool {
        column
            .windows
            .iter()
            .all(|w| self.born_floating.contains(w))
    }
    fn insert_with_focus_inner(
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
            self.default_width
                .resolve(self.viewport.width)?
                .max(constraints.min.width)
                + self.gap,
        )?;
        let index = self
            .focus
            .and_then(|w| self.column_index(w))
            .map_or(self.columns.len(), |i| i + 1);
        let group = GroupId(self.next_group);
        self.next_group = self
            .next_group
            .checked_add(1)
            .ok_or(LayoutError::CapacityExceeded)?;
        self.members.insert(window, constraints);
        self.columns.insert(
            index,
            Column {
                group,
                windows: vec![window],
                width: self.default_width,
                mode: ColumnMode::Stack,
                active: window,
                scroll_y: 0.0,
            },
        );
        // Seed only the new canvas member. Existing canvas positions survive strip operations.
        let strip = self.strip_placements();
        let mut rect = strip.iter().find(|p| p.window == window).unwrap().rect;
        if self.mode == LayoutMode::Canvas
            && let Some(r) = self
                .focus
                .and_then(|w| self.canvas.get(&self.group_for(w)?))
                .map(|g| g.rect)
        {
            rect.x = (r.x + r.width + self.gap).min(MAX_COORD - rect.width);
            rect.y = r.y;
        }
        self.canvas.insert(
            group,
            CanvasGroup {
                rect,
                scroll_y: 0.0,
            },
        );
        if activate || self.focus.is_none() {
            self.focus_window(window)
        } else {
            if !self.fullscreen.is_empty() {
                self.reveal_focus();
            }
            Ok(())
        }
    }
    pub fn remove(&mut self, window: WindowId) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.remove_inner(window))
    }
    fn remove_inner(&mut self, window: WindowId) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        let anchor = if self.mode == LayoutMode::Strip && self.focus != Some(window) {
            self.focus.map(|w| {
                (
                    w,
                    self.projected_placements()
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
            self.canvas.remove(&c.group);
            self.columns.remove(index);
        } else if c.active == window {
            c.active = c
                .windows
                .iter()
                .copied()
                .find(|w| {
                    !self.minimized.contains(w)
                        && !self.floating.contains_key(w)
                        && !self.fullscreen.contains(w)
                })
                .or_else(|| {
                    c.windows
                        .iter()
                        .copied()
                        .find(|w| !self.minimized.contains(w))
                })
                .unwrap_or(c.windows[0]);
        }
        self.members.remove(&window);
        self.minimized.remove(&window);
        self.maximized.remove(&window);
        self.fullscreen.remove(&window);
        self.floating.remove(&window);
        self.born_floating.remove(&window);
        self.history.retain(|w| *w != window);
        if self.focus == Some(window) {
            self.focus = None;
            if let Some(next) = self.replacement_focus(index) {
                self.focus_window(next)?;
            }
        }
        if let Some((w, old_x)) = anchor {
            let new_x = self
                .projected_placements()
                .into_iter()
                .find(|p| p.window == w)
                .unwrap()
                .rect
                .x;
            self.strip_camera.position.x =
                (self.strip_camera.position.x + new_x - old_x).clamp(-MAX_COORD, MAX_COORD);
            self.reveal_focus();
        }
        self.reveal_focused_canvas_page();
        if self.is_empty() {
            self.strip_camera = Camera::default();
            self.canvas_camera = Camera::default();
        }
        Ok(())
    }
    /// Minimized members reserve their geometry and group slot. A real restore
    /// activates and reveals the member; repeated requests leave focus unchanged.
    pub fn set_minimized(&mut self, window: WindowId, minimized: bool) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.set_minimized_inner(window, minimized))
    }
    fn set_minimized_inner(
        &mut self,
        window: WindowId,
        minimized: bool,
    ) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        if self.is_minimized(window) == minimized {
            return Ok(());
        }
        if !minimized {
            self.minimized.remove(&window);
            return self.focus_window(window);
        }
        self.minimized.insert(window);
        self.history.retain(|w| *w != window);
        if self.columns[index].active == window {
            let c = &mut self.columns[index];
            c.active = c
                .windows
                .iter()
                .copied()
                .find(|w| {
                    !self.minimized.contains(w)
                        && !self.floating.contains_key(w)
                        && !self.fullscreen.contains(w)
                })
                .or_else(|| {
                    c.windows
                        .iter()
                        .copied()
                        .find(|w| !self.minimized.contains(w))
                })
                .unwrap_or(window);
        }
        if self.focus == Some(window) {
            self.focus = None;
            if let Some(next) = self.replacement_focus(index) {
                self.focus_window(next)?;
            }
        }
        Ok(())
    }
    /// Unknown and removed identities are never minimized.
    pub fn is_minimized(&self, window: WindowId) -> bool {
        self.minimized.contains(&window)
    }
    /// Return the active usable viewport size in world units when the client's
    /// committed bounds permit that exact maximized size. This does not mutate
    /// the layout and can also preflight a window that has not been admitted yet.
    pub fn maximize_size(&self, constraints: SizeConstraints) -> Result<Size, LayoutError> {
        if !constraints.valid() {
            return Err(LayoutError::InvalidConstraints);
        }
        if [self.strip_camera, self.canvas_camera]
            .iter()
            .any(|camera| maximized_rect(self.viewport, *camera).is_none())
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let rect =
            maximized_rect(self.viewport, self.camera()).ok_or(LayoutError::InvalidGeometry)?;
        let size = Size {
            width: rect.width,
            height: rect.height,
        };
        if size.width < constraints.min.width
            || size.height < constraints.min.height
            || constraints
                .max
                .is_some_and(|max| size.width > max.width || size.height > max.height)
        {
            return Err(LayoutError::InvalidConstraints);
        }
        Ok(size)
    }
    fn clear_incompatible_maximized(&mut self) {
        let incompatible: Vec<_> = self
            .maximized
            .iter()
            .copied()
            .filter(|window| self.maximize_size(self.members[window]).is_err())
            .collect();
        for window in incompatible {
            self.maximized.remove(&window);
        }
    }
    /// Present a member across the usable viewport without changing its saved
    /// slot, constraints, group, focus, or minimized state. Hidden tabs stay hidden.
    /// Both saved camera views must fit the supported coordinate bounds.
    pub fn set_maximized(&mut self, window: WindowId, maximized: bool) -> Result<(), LayoutError> {
        if !self.members.contains_key(&window) {
            return Err(LayoutError::UnknownWindow);
        }
        if maximized {
            self.maximize_size(self.members[&window])?;
            self.maximized.insert(window);
        } else {
            self.maximized.remove(&window);
        }
        Ok(())
    }
    /// Unknown and removed identities are never maximized.
    pub fn is_maximized(&self, window: WindowId) -> bool {
        self.maximized.contains(&window)
    }
    /// Full output page size in world coordinates. Smaller maximum hints are
    /// allowed: the compositor centers committed content inside an opaque page.
    /// A minimum larger than the page cannot be represented and is declined.
    /// Successful viewport, zoom, mode and constraint updates clear fullscreen
    /// if either saved mode can no longer satisfy that minimum.
    pub fn fullscreen_size(&self, constraints: SizeConstraints) -> Result<Size, LayoutError> {
        self.fullscreen_size_for(constraints, self.mode)
    }
    fn fullscreen_size_for(
        &self,
        constraints: SizeConstraints,
        mode: LayoutMode,
    ) -> Result<Size, LayoutError> {
        if !constraints.valid() {
            return Err(LayoutError::InvalidConstraints);
        }
        let camera = if mode == LayoutMode::Strip {
            self.strip_camera
        } else {
            self.canvas_camera
        };
        let size = Size {
            width: self.viewport.width / camera.zoom,
            height: self.viewport.height / camera.zoom,
        };
        if !valid_size(size) {
            return Err(LayoutError::InvalidGeometry);
        }
        if constraints.min.width > size.width || constraints.min.height > size.height {
            return Err(LayoutError::InvalidConstraints);
        }
        Ok(size)
    }
    /// Reserve a separate output-sized page immediately after the saved strip
    /// column, or in a canvas page bank beyond every saved group's right edge.
    /// Canvas pages can shift when groups are added, removed or moved; saved
    /// ordinary geometry remains untouched. Saved ordinary slots remain
    /// reserved even when every member is fullscreen. Multiple pages follow
    /// original member order. This does not activate, minimize or maximize a member.
    pub fn set_fullscreen(
        &mut self,
        window: WindowId,
        fullscreen: bool,
    ) -> Result<(), LayoutError> {
        if !self.members.contains_key(&window) {
            return Err(LayoutError::UnknownWindow);
        }
        if self.is_fullscreen(window) == fullscreen {
            return Ok(());
        }
        let mut candidate = self.clone();
        if fullscreen {
            candidate.fullscreen.insert(window);
        } else {
            candidate.fullscreen.remove(&window);
            if candidate.focus == Some(window) && !candidate.is_floating(window) {
                let index = candidate.column_index(window).unwrap();
                candidate.columns[index].active = window;
            }
        }
        candidate.check_fullscreen_geometry()?;
        candidate.reveal_focus();
        *self = candidate;
        Ok(())
    }
    pub fn is_fullscreen(&self, window: WindowId) -> bool {
        self.fullscreen.contains(&window)
    }
    /// Enter the floating layer without changing saved tiling membership. An
    /// explicit initial rectangle is used only on entry; repeated entry preserves
    /// the stored rectangle. All coordinates and sizes are world logical units.
    pub fn set_floating(
        &mut self,
        window: WindowId,
        floating: bool,
        initial: Option<Rect>,
    ) -> Result<(), LayoutError> {
        if !self.members.contains_key(&window) {
            return Err(LayoutError::UnknownWindow);
        }
        if self.is_floating(window) == floating {
            return Ok(());
        }
        let mut candidate = self.clone();
        if floating {
            let camera = self.camera();
            let size = self.members[&window].size(Size {
                width: self.viewport.width / camera.zoom * 0.6,
                height: self.viewport.height / camera.zoom * 0.6,
            });
            let rect = initial.unwrap_or(Rect {
                x: camera.position.x + (self.viewport.width / camera.zoom - size.width) / 2.0,
                y: camera.position.y + (self.viewport.height / camera.zoom - size.height) / 2.0,
                width: size.width,
                height: size.height,
            });
            candidate.store_floating_rect(window, rect)?;
        } else {
            candidate.floating.remove(&window);
            let materializing = candidate.born_floating.remove(&window);
            if materializing {
                candidate.seed_materialized_canvas_group(window)?;
            }
            candidate.check_strip_extent(
                candidate.viewport.width,
                candidate.gap,
                None,
                None,
                0.0,
            )?;
            if candidate.focus == Some(window) && !candidate.is_fullscreen(window) {
                let index = candidate.column_index(window).unwrap();
                candidate.columns[index].active = window;
            }
            if materializing {
                for mode in [LayoutMode::Strip, LayoutMode::Canvas] {
                    candidate.fullscreen_projection(mode)?;
                }
            }
        }
        candidate.check_fullscreen_geometry()?;
        candidate.reveal_focus();
        *self = candidate;
        Ok(())
    }
    fn seed_materialized_canvas_group(&mut self, window: WindowId) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        let width = self
            .default_width
            .resolve(self.viewport.width)?
            .max(self.members[&window].min.width);
        // A born-floating window has never had a tiled canvas slot. Seed one
        // beside the nearest saved ordinary column, preferring the preceding
        // column on equal distance. Its birth and current floating rects are
        // unrelated to this new tiled geometry.
        let anchor = self
            .columns
            .iter()
            .enumerate()
            .filter(|(i, column)| *i != index && !self.dormant_column(column))
            .min_by_key(|(i, _)| i.abs_diff(index))
            .map(|(_, column)| self.canvas[&column.group].rect);
        let rect = Rect {
            x: anchor.map_or(0.0, |r| (r.x + r.width + self.gap).min(MAX_COORD - width)),
            y: anchor.map_or(0.0, |r| r.y),
            width,
            height: self.viewport.height,
        };
        if !bounded_rect(rect) {
            return Err(LayoutError::InvalidGeometry);
        }
        let group = self.columns[index].group;
        self.columns[index].width = self.default_width;
        self.columns[index].scroll_y = 0.0;
        self.canvas.insert(
            group,
            CanvasGroup {
                rect,
                scroll_y: 0.0,
            },
        );
        Ok(())
    }
    pub fn member_constraints(&self, window: WindowId) -> Option<SizeConstraints> {
        self.members.get(&window).copied()
    }
    pub fn is_floating(&self, window: WindowId) -> bool {
        self.floating.contains_key(&window)
    }
    pub fn floating_rect(&self, window: WindowId) -> Option<Rect> {
        self.floating.get(&window).copied()
    }
    /// Resize or move the saved floating rectangle, including while an overlay
    /// or minimization hides it. Constraints clamp size; invalid edges reject
    /// the entire edit without modifying the previous state.
    pub fn set_floating_rect(&mut self, window: WindowId, rect: Rect) -> Result<(), LayoutError> {
        if !self.members.contains_key(&window) {
            return Err(LayoutError::UnknownWindow);
        }
        if !self.is_floating(window) {
            return Err(LayoutError::FloatingWindow);
        }
        self.edit_fullscreen(|candidate| {
            candidate.store_floating_rect(window, rect)?;
            if candidate.focus.is_some_and(|w| candidate.is_fullscreen(w)) {
                candidate.reveal_focus();
            }
            Ok(())
        })
    }
    fn store_floating_rect(&mut self, window: WindowId, mut rect: Rect) -> Result<(), LayoutError> {
        if !valid_rect(rect) {
            return Err(LayoutError::InvalidGeometry);
        }
        let size = self.members[&window].size(Size {
            width: rect.width,
            height: rect.height,
        });
        rect.width = size.width;
        rect.height = size.height;
        if !bounded_rect(rect) {
            return Err(LayoutError::InvalidGeometry);
        }
        self.floating.insert(window, rect);
        Ok(())
    }
    fn edit_fullscreen<R>(
        &mut self,
        edit: impl FnOnce(&mut Self) -> Result<R, LayoutError>,
    ) -> Result<R, LayoutError> {
        if self.fullscreen.is_empty() && self.floating.is_empty() {
            return edit(self);
        }
        let mut candidate = self.clone();
        let result = edit(&mut candidate)?;
        candidate.check_fullscreen_geometry()?;
        *self = candidate;
        Ok(result)
    }
    fn clear_incompatible_fullscreen(&mut self) {
        let incompatible: Vec<_> = self
            .fullscreen
            .iter()
            .copied()
            .filter(|window| {
                [LayoutMode::Strip, LayoutMode::Canvas]
                    .into_iter()
                    .any(|mode| {
                        self.fullscreen_size_for(self.members[window], mode)
                            == Err(LayoutError::InvalidConstraints)
                    })
            })
            .collect();
        for window in incompatible {
            self.fullscreen.remove(&window);
            if self.focus == Some(window) && !self.is_floating(window) {
                let index = self.column_index(window).unwrap();
                self.columns[index].active = window;
            }
        }
    }
    fn check_fullscreen_geometry(&self) -> Result<(), LayoutError> {
        if self.fullscreen.is_empty() && self.floating.is_empty() {
            return Ok(());
        }
        for mode in [LayoutMode::Strip, LayoutMode::Canvas] {
            self.fullscreen_projection(mode)?;
        }
        Ok(())
    }
    fn projected_placements(&self) -> Vec<Placement> {
        if self.fullscreen.is_empty() && self.floating.is_empty() {
            self.base_placements()
        } else {
            self.fullscreen_projection(self.mode)
                .expect("validated floating/fullscreen projection")
        }
    }
    fn fullscreen_projection(&self, mode: LayoutMode) -> Result<Vec<Placement>, LayoutError> {
        let mut result = Vec::with_capacity(self.len());
        let mut strip_x = 0.0;
        // Canvas pages occupy a separate bank beyond every saved group. Using
        // the complete saved geometry also reserves minimized/hidden groups and
        // prevents a newly admitted ordinary group from occupying a live page.
        let mut canvas_page_x = self
            .columns
            .iter()
            .filter(|c| !self.dormant_column(c))
            .map(|c| self.canvas[&c.group].rect)
            .map(|rect| rect.x + rect.width)
            .reduce(f64::max)
            .unwrap_or(0.0)
            + self.gap;
        for c in &self.columns {
            let saved_width = c
                .windows
                .iter()
                .map(|w| self.members[w].min.width)
                .fold(c.width.resolve(self.viewport.width)?, f64::max);
            let group = self.canvas[&c.group];
            let container = if mode == LayoutMode::Strip {
                Rect {
                    x: strip_x,
                    y: 0.0,
                    width: saved_width,
                    height: self.viewport.height,
                }
            } else {
                group.rect
            };
            let ordinary: Vec<_> = c
                .windows
                .iter()
                .copied()
                .filter(|w| !self.is_fullscreen(*w) && !self.is_floating(*w))
                .collect();
            let active = ordinary
                .iter()
                .copied()
                .find(|w| *w == c.active && !self.is_minimized(*w))
                .or_else(|| ordinary.iter().copied().find(|w| !self.is_minimized(*w)));
            let heights = allocate_heights(
                &ordinary,
                &self.members,
                (container.height - self.gap * ordinary.len().saturating_sub(1) as f64).max(0.0),
            );
            let scroll = if mode == LayoutMode::Strip {
                c.scroll_y
            } else {
                group.scroll_y
            };
            let mut y = container.y - scroll;
            for (i, window) in ordinary.into_iter().enumerate() {
                let size = self.members[&window].size(Size {
                    width: container.width,
                    height: if c.mode == ColumnMode::Tabs {
                        container.height
                    } else {
                        heights[i]
                    },
                });
                result.push(Placement {
                    window,
                    rect: Rect {
                        x: container.x,
                        y: if c.mode == ColumnMode::Tabs {
                            container.y
                        } else {
                            y
                        },
                        width: size.width,
                        height: size.height,
                    },
                    clip: container,
                    visible: !self.is_minimized(window)
                        && (c.mode == ColumnMode::Stack || active == Some(window)),
                    focused: self.focus == Some(window),
                });
                y += size.height + self.gap;
            }
            let mut page_x = if mode == LayoutMode::Strip {
                if self.dormant_column(c) {
                    container.x
                } else {
                    container.x + container.width + self.gap
                }
            } else {
                canvas_page_x
            };
            for window in c.windows.iter().copied().filter(|w| self.is_fullscreen(*w)) {
                let size = self.fullscreen_size_for(self.members[&window], mode)?;
                let page = Rect {
                    x: page_x,
                    y: container.y,
                    width: size.width,
                    height: size.height,
                };
                result.push(Placement {
                    window,
                    rect: page,
                    clip: page,
                    visible: !self.is_minimized(window),
                    focused: self.focus == Some(window),
                });
                page_x += size.width + self.gap;
            }
            strip_x = if c.windows.iter().any(|w| self.is_fullscreen(*w)) {
                page_x
            } else if self.dormant_column(c) {
                container.x
            } else {
                container.x + container.width + self.gap
            };
            if mode == LayoutMode::Canvas {
                canvas_page_x = page_x;
            }
        }
        for (&window, &rect) in &self.floating {
            if !self.is_fullscreen(window) {
                result.push(Placement {
                    window,
                    rect,
                    clip: rect,
                    visible: !self.is_minimized(window),
                    focused: self.focus == Some(window),
                });
            }
        }
        if result.iter().any(|p| {
            !valid_rect(p.rect)
                || !valid_rect(p.clip)
                || p.rect.x + p.rect.width > MAX_COORD
                || p.rect.y + p.rect.height > MAX_COORD
                || p.clip.x + p.clip.width > MAX_COORD
                || p.clip.y + p.clip.height > MAX_COORD
        }) {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(result)
    }
    fn ordinary_active(&self, column: &Column) -> Option<WindowId> {
        let selectable = |window: WindowId| {
            !self.is_minimized(window) && !self.is_floating(window) && !self.is_fullscreen(window)
        };
        column
            .windows
            .iter()
            .copied()
            .find(|w| *w == column.active && selectable(*w))
            .or_else(|| column.windows.iter().copied().find(|w| selectable(*w)))
    }
    fn replacement_focus(&self, column: usize) -> Option<WindowId> {
        self.history
            .iter()
            .rev()
            .copied()
            .find(|w| !self.is_minimized(*w))
            .or_else(|| {
                self.columns
                    .iter()
                    .skip(column)
                    .chain(self.columns.iter().take(column))
                    .find_map(|c| (!self.is_minimized(c.active)).then_some(c.active))
            })
    }
    pub fn focus_window(&mut self, window: WindowId) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.focus_window_inner(window))
    }
    fn focus_window_inner(&mut self, window: WindowId) -> Result<(), LayoutError> {
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        if self.is_minimized(window) {
            return Err(LayoutError::MinimizedWindow);
        }
        if (!self.is_floating(window) && !self.is_fullscreen(window))
            || self.is_minimized(self.columns[index].active)
        {
            self.columns[index].active =
                self.ordinary_active(&self.columns[index]).unwrap_or(window);
            if !self.is_floating(window) && !self.is_fullscreen(window) {
                self.columns[index].active = window;
            }
        }
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
        self.edit_fullscreen(|candidate| candidate.set_constraints_inner(window, constraints))
    }
    fn set_constraints_inner(
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
        if let Some(rect) = self.floating_rect(window) {
            self.store_floating_rect(window, rect)?;
        }
        self.clear_incompatible_maximized();
        self.clear_incompatible_fullscreen();
        let group = self.columns[self.column_index(window).unwrap()].group;
        let minimum = self
            .columns
            .iter()
            .find(|c| c.group == group)
            .unwrap()
            .windows
            .iter()
            .map(|w| self.members[w].min.width)
            .fold(1.0, f64::max);
        self.canvas.get_mut(&group).unwrap().rect.width =
            self.canvas[&group].rect.width.max(minimum);
        self.reveal_focus();
        Ok(())
    }
    /// First conversion seeds from current strip geometry. First return sorts groups
    /// by x, y, stable identity; subsequent transitions restore saved cameras/geometry.
    /// A page-coordinate overflow leaves the mode unchanged. Reconfiguration reports
    /// that refusal as an error. Incompatible fullscreen minima clear fullscreen.
    pub fn set_mode(&mut self, mode: LayoutMode) {
        let _ = self.edit_fullscreen(|candidate| {
            candidate.set_mode_inner(mode);
            Ok(())
        });
    }
    fn set_mode_inner(&mut self, mode: LayoutMode) {
        if self.mode == mode {
            return;
        }
        let mut reveal = false;
        if mode == LayoutMode::Canvas && !self.canvas_initialized {
            let mut x = 0.0;
            for c in &self.columns {
                if self.dormant_column(c) {
                    continue;
                }
                let width = c
                    .windows
                    .iter()
                    .map(|w| self.members[w].min.width)
                    .fold(c.width.resolve(self.viewport.width).unwrap(), f64::max);
                self.canvas.insert(
                    c.group,
                    CanvasGroup {
                        rect: Rect {
                            x,
                            y: 0.0,
                            width,
                            height: self.viewport.height,
                        },
                        scroll_y: c.scroll_y,
                    },
                );
                x += width + self.gap;
            }
            self.canvas_initialized = true;
            reveal = true;
        } else if mode == LayoutMode::Strip && !self.strip_converted {
            self.columns.sort_by(|a, b| {
                let a_rect = self.canvas[&a.group].rect;
                let b_rect = self.canvas[&b.group].rect;
                a_rect
                    .x
                    .total_cmp(&b_rect.x)
                    .then(a_rect.y.total_cmp(&b_rect.y))
                    .then(a.group.cmp(&b.group))
            });
            self.strip_converted = true;
            reveal = true;
        }
        self.mode = mode;
        self.clear_incompatible_maximized();
        self.clear_incompatible_fullscreen();
        if reveal {
            self.reveal_focus();
        }
    }
    pub fn group_for(&self, window: WindowId) -> Option<GroupId> {
        self.column_index(window).map(|i| self.columns[i].group)
    }
    /// Groups with a selectable member. Fully minimized groups retain their
    /// stored geometry but have no active member to expose to interaction code.
    pub fn canvas_groups(&self) -> Vec<GroupGeometry> {
        self.columns
            .iter()
            .filter_map(|c| {
                self.ordinary_active(c).map(|active| GroupGeometry {
                    id: c.group,
                    rect: self.canvas[&c.group].rect,
                    active,
                    mode: c.mode,
                })
            })
            .collect()
    }
    /// Resize/move the containing canvas group. Client minima are retained by its
    /// local layout; a member maximum constrains the member rather than its group.
    pub fn set_canvas_rect(&mut self, window: WindowId, rect: Rect) -> Result<(), LayoutError> {
        if self.is_floating(window) {
            return Err(LayoutError::FloatingWindow);
        }
        let group = self.group_for(window).ok_or(LayoutError::UnknownWindow)?;
        self.set_canvas_group_rect(group, rect)
    }
    pub fn set_canvas_group_rect(&mut self, group: GroupId, rect: Rect) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.set_canvas_group_rect_inner(group, rect))
    }
    fn set_canvas_group_rect_inner(
        &mut self,
        group: GroupId,
        mut rect: Rect,
    ) -> Result<(), LayoutError> {
        if !valid_rect(rect) {
            return Err(LayoutError::InvalidGeometry);
        }
        let column = self
            .columns
            .iter()
            .find(|c| c.group == group)
            .ok_or(LayoutError::UnknownGroup)?;
        rect.width = column
            .windows
            .iter()
            .map(|w| self.members[w].min.width)
            .fold(rect.width, f64::max);
        self.canvas.get_mut(&group).unwrap().rect = rect;
        self.reveal_focused_canvas_page();
        Ok(())
    }
    /// Explicit tab navigation; hidden tabs are excluded from spatial focus.
    pub fn cycle_tab(&mut self, forward: bool) -> Result<Option<WindowId>, LayoutError> {
        let Some(window) = self.focus else {
            return Ok(None);
        };
        if self.is_floating(window) || self.is_fullscreen(window) {
            return Ok(Some(window));
        }
        let c = &self.columns[self.column_index(window).unwrap()];
        let windows: Vec<_> = c
            .windows
            .iter()
            .copied()
            .filter(|w| !self.is_minimized(*w) && !self.is_floating(*w) && !self.is_fullscreen(*w))
            .collect();
        let index = windows.iter().position(|w| *w == window).unwrap();
        let next = windows[if forward {
            (index + 1) % windows.len()
        } else {
            (index + windows.len() - 1) % windows.len()
        }];
        self.focus_window(next)?;
        Ok(Some(next))
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
        self.edit_fullscreen(|candidate| candidate.set_column_width_inner(window, policy))
    }
    fn set_column_width_inner(
        &mut self,
        window: WindowId,
        policy: WidthPolicy,
    ) -> Result<(), LayoutError> {
        if self.is_floating(window) {
            return Err(LayoutError::FloatingWindow);
        }
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
        self.edit_fullscreen(|candidate| candidate.set_column_mode_inner(window, mode))
    }
    fn set_column_mode_inner(
        &mut self,
        window: WindowId,
        mode: ColumnMode,
    ) -> Result<(), LayoutError> {
        if self.is_floating(window) {
            return Err(LayoutError::FloatingWindow);
        }
        let index = self
            .column_index(window)
            .ok_or(LayoutError::UnknownWindow)?;
        self.columns[index].mode = mode;
        self.reveal_focus();
        Ok(())
    }
    /// Move a member into another column, with no duplicate or orphan intermediate state.
    pub fn join_column(&mut self, window: WindowId, target: WindowId) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.join_column_inner(window, target))
    }
    fn join_column_inner(&mut self, window: WindowId, target: WindowId) -> Result<(), LayoutError> {
        if self.is_minimized(window) || self.is_minimized(target) {
            return Err(LayoutError::MinimizedWindow);
        }
        if self.is_floating(window) || self.is_floating(target) {
            return Err(LayoutError::FloatingWindow);
        }
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
                .iter()
                .copied()
                .find(|w| !self.minimized.contains(w))
                .or_else(|| self.columns[source].windows.first().copied())
                .unwrap_or(window);
        }
        self.columns[dest].windows.push(window);
        self.columns[dest].active = window;
        let dest_group = self.columns[dest].group;
        self.canvas.get_mut(&dest_group).unwrap().rect.width = self.canvas[&dest_group]
            .rect
            .width
            .max(self.members[&window].min.width);
        if self.columns[source].windows.is_empty() {
            self.canvas.remove(&self.columns[source].group);
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
        if let Some(rect) = self.floating_rect(window) {
            self.set_floating_rect(
                window,
                Rect {
                    width: rect.width + delta,
                    ..rect
                },
            )?;
            self.reveal_focus();
            return Ok(());
        }
        if self.mode == LayoutMode::Canvas {
            let r = self.canvas[&self.group_for(window).unwrap()].rect;
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
        self.edit_fullscreen(|candidate| candidate.move_focused_inner(direction))
    }
    fn move_focused_inner(&mut self, direction: Direction) -> Result<(), LayoutError> {
        let Some(window) = self.focus else {
            return Ok(());
        };
        if let Some(mut rect) = self.floating_rect(window) {
            match direction {
                Direction::Left => rect.x -= 32.0,
                Direction::Right => rect.x += 32.0,
                Direction::Up => rect.y -= 32.0,
                Direction::Down => rect.y += 32.0,
            }
            self.store_floating_rect(window, rect)?;
            self.reveal_focus();
            return Ok(());
        }
        if self.mode == LayoutMode::Canvas {
            let mut r = self.canvas[&self.group_for(window).unwrap()].rect;
            match direction {
                Direction::Left => r.x -= 32.0,
                Direction::Right => r.x += 32.0,
                Direction::Up => r.y -= 32.0,
                Direction::Down => r.y += 32.0,
            }
            self.set_canvas_rect(window, r)?;
        } else {
            let old_x = self
                .projected_placements()
                .into_iter()
                .find(|p| p.window == window)
                .unwrap()
                .rect
                .x;
            let i = self.column_index(window).unwrap();
            match direction {
                Direction::Left => {
                    if let Some(next) = (0..i)
                        .rev()
                        .find(|j| !self.is_minimized(self.columns[*j].active))
                    {
                        self.columns.swap(i, next);
                    }
                }
                Direction::Right => {
                    if let Some(next) = (i + 1..self.columns.len())
                        .find(|j| !self.is_minimized(self.columns[*j].active))
                    {
                        self.columns.swap(i, next);
                    }
                }
                Direction::Up | Direction::Down => {
                    let c = &mut self.columns[i];
                    let j = c.windows.iter().position(|w| *w == window).unwrap();
                    let next = if direction == Direction::Up {
                        (0..j)
                            .rev()
                            .find(|k| !self.minimized.contains(&c.windows[*k]))
                    } else {
                        (j + 1..c.windows.len()).find(|k| !self.minimized.contains(&c.windows[*k]))
                    };
                    if let Some(next) = next {
                        c.windows.swap(j, next);
                    }
                }
            }
            let new_x = self
                .projected_placements()
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
    fn virtual_strip_columns(&self) -> Vec<(Vec<WindowId>, Option<WindowId>)> {
        let mut result = Vec::with_capacity(self.columns.len() + self.fullscreen.len());
        for column in &self.columns {
            let ordinary: Vec<_> = column
                .windows
                .iter()
                .copied()
                .filter(|w| !self.is_fullscreen(*w) && !self.is_floating(*w))
                .collect();
            let active = ordinary
                .iter()
                .copied()
                .find(|w| *w == column.active && !self.is_minimized(*w))
                .or_else(|| ordinary.iter().copied().find(|w| !self.is_minimized(*w)));
            result.push((ordinary, active));
            for window in column
                .windows
                .iter()
                .copied()
                .filter(|w| self.is_fullscreen(*w))
            {
                result.push((vec![window], (!self.is_minimized(window)).then_some(window)));
            }
        }
        result
    }
    pub fn focus_direction(
        &mut self,
        direction: Direction,
    ) -> Result<Option<WindowId>, LayoutError> {
        let Some(window) = self.focus else {
            return Ok(None);
        };
        let next = if self.mode == LayoutMode::Strip
            && (!self.is_floating(window) || self.is_fullscreen(window))
        {
            // Each saved column retains one ordinary virtual column, followed
            // by one single-member virtual column for each fullscreen page.
            // Left/right use column order, never geometric angle scoring.
            let columns = self.virtual_strip_columns();
            let i = columns
                .iter()
                .position(|(members, _)| members.contains(&window))
                .unwrap();
            let (members, _) = &columns[i];
            let j = members.iter().position(|w| *w == window).unwrap();
            match direction {
                Direction::Left => columns[..i].iter().rev().find_map(|(_, active)| *active),
                Direction::Right => columns[i + 1..].iter().find_map(|(_, active)| *active),
                Direction::Up => members[..j]
                    .iter()
                    .rev()
                    .copied()
                    .find(|w| !self.is_minimized(*w)),
                Direction::Down => members[j + 1..]
                    .iter()
                    .copied()
                    .find(|w| !self.is_minimized(*w)),
            }
        } else {
            self.spatial_neighbor(window, direction, false)
        };
        let next =
            next.or_else(|| self.spatial_neighbor(window, direction, !self.is_floating(window)));
        if let Some(next) = next {
            self.focus_window(next)?;
        }
        Ok(next)
    }
    fn spatial_neighbor(
        &self,
        window: WindowId,
        direction: Direction,
        floats_only: bool,
    ) -> Option<WindowId> {
        let placements = self.projected_placements();
        let source = placements.iter().find(|p| p.window == window).unwrap().rect;
        let origin = center(source);
        placements
            .iter()
            .filter(|p| {
                p.window != window && p.visible && (!floats_only || self.is_floating(p.window))
            })
            .filter_map(|candidate| {
                let r = candidate.rect;
                let p = center(r);
                let (forward, perpendicular, edge) = match direction {
                    Direction::Left => (
                        origin.x - p.x,
                        (origin.y - p.y).abs(),
                        source.x - (r.x + r.width),
                    ),
                    Direction::Right => (
                        p.x - origin.x,
                        (origin.y - p.y).abs(),
                        r.x - (source.x + source.width),
                    ),
                    Direction::Up => (
                        origin.y - p.y,
                        (origin.x - p.x).abs(),
                        source.y - (r.y + r.height),
                    ),
                    Direction::Down => (
                        p.y - origin.y,
                        (origin.x - p.x).abs(),
                        r.y - (source.y + source.height),
                    ),
                };
                let forward = if forward == 0.0
                    && perpendicular == 0.0
                    && self.is_floating(candidate.window)
                    && direction == Direction::Up
                {
                    1.0
                } else {
                    forward
                };
                (forward > 0.0).then_some((
                    candidate.window,
                    perpendicular / forward,
                    edge.max(0.0),
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
    }
    /// All members, including hidden tabs and offscreen content, in deterministic scene order.
    pub fn placements(&self) -> Vec<Placement> {
        let mut placements = self.projected_placements();
        if !self.maximized.is_empty() {
            let rect =
                maximized_rect(self.viewport, self.camera()).expect("maximized viewport invariant");
            for placement in &mut placements {
                if self.is_maximized(placement.window) && !self.is_fullscreen(placement.window) {
                    placement.rect = rect;
                    placement.clip = rect;
                }
            }
        }
        placements
    }
    fn base_placements(&self) -> Vec<Placement> {
        if self.mode == LayoutMode::Strip {
            self.strip_placements()
        } else {
            let mut placements = Vec::with_capacity(self.len());
            for c in &self.columns {
                let group = self.canvas[&c.group];
                let heights = allocate_heights(
                    &c.windows,
                    &self.members,
                    (group.rect.height - self.gap * c.windows.len().saturating_sub(1) as f64)
                        .max(0.0),
                );
                let mut y = group.rect.y - group.scroll_y;
                for (i, window) in c.windows.iter().enumerate() {
                    let size = self.members[window].size(Size {
                        width: group.rect.width,
                        height: if c.mode == ColumnMode::Tabs {
                            group.rect.height
                        } else {
                            heights[i]
                        },
                    });
                    placements.push(Placement {
                        clip: group.rect,
                        window: *window,
                        rect: Rect {
                            x: group.rect.x,
                            y: if c.mode == ColumnMode::Tabs {
                                group.rect.y
                            } else {
                                y
                            },
                            width: size.width,
                            height: size.height,
                        },
                        visible: !self.is_minimized(*window)
                            && (c.mode == ColumnMode::Stack || *window == c.active),
                        focused: self.focus == Some(*window),
                    });
                    y += size.height + self.gap;
                }
            }
            placements
        }
    }

    pub fn visible_placements(&self) -> Vec<Placement> {
        let camera = self.camera();
        self.placements()
            .into_iter()
            .filter(|p| {
                let r = camera.rect_to_view(p.rect);
                let clip = camera.rect_to_view(p.clip);
                p.visible
                    && r.x < clip.x + clip.width
                    && r.y < clip.y + clip.height
                    && r.x + r.width > clip.x
                    && r.y + r.height > clip.y
                    && clip.x < self.viewport.width
                    && clip.y < self.viewport.height
                    && clip.x + clip.width > 0.0
                    && clip.y + clip.height > 0.0
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
        let hits = |p: &&Placement| p.visible && p.rect.contains(world) && p.clip.contains(world);
        placements
            .iter()
            .filter(hits)
            .find(|p| self.is_floating(p.window) && p.focused)
            .or_else(|| {
                placements
                    .iter()
                    .rev()
                    .filter(hits)
                    .find(|p| self.is_floating(p.window))
            })
            .or_else(|| placements.iter().filter(hits).find(|p| p.focused))
            .or_else(|| {
                placements
                    .iter()
                    .rev()
                    .find(|p| p.visible && p.rect.contains(world) && p.clip.contains(world))
            })
            .map(|p| p.window)
    }
    /// Shift every canvas location and its camera atomically. Returned delta must also be
    /// applied by the caller to any active gesture, animation, or external spatial index.
    /// Tiled strip coordinates are prefix sums and are intentionally unaffected.
    /// Floating coordinates are shared by both modes and shift with the canvas.
    pub fn rebase_canvas(&mut self, delta: Point) -> Result<(), LayoutError> {
        self.edit_fullscreen(|candidate| candidate.rebase_canvas_inner(delta))
    }
    fn rebase_canvas_inner(&mut self, delta: Point) -> Result<(), LayoutError> {
        if !valid_point(delta) {
            return Err(LayoutError::InvalidGeometry);
        }
        let shift = |p: Point| Point {
            x: p.x - delta.x,
            y: p.y - delta.y,
        };
        let camera = shift(self.canvas_camera.position);
        if !valid_point(camera)
            || (!self.maximized.is_empty()
                && maximized_rect(
                    self.viewport,
                    Camera {
                        position: camera,
                        ..self.canvas_camera
                    },
                )
                .is_none())
            || self.floating.values().any(|r| {
                !bounded_rect(Rect {
                    x: r.x - delta.x,
                    y: r.y - delta.y,
                    ..*r
                })
            })
            || self.canvas.values().any(|g| {
                !valid_point(shift(Point {
                    x: g.rect.x,
                    y: g.rect.y,
                }))
            })
        {
            return Err(LayoutError::InvalidGeometry);
        }
        for group in self.canvas.values_mut() {
            group.rect.x -= delta.x;
            group.rect.y -= delta.y;
        }
        for rect in self.floating.values_mut() {
            rect.x -= delta.x;
            rect.y -= delta.y;
        }
        self.canvas_camera.position = camera;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.default_width.resolve(self.viewport.width).is_err() {
            return Err("invalid default width");
        }
        let flattened: Vec<_> = self
            .columns
            .iter()
            .flat_map(|c| c.windows.iter().copied())
            .collect();
        let unique: BTreeSet<_> = flattened.iter().copied().collect();
        if unique.len() != flattened.len()
            || unique.len() != self.members.len()
            || unique.iter().any(|w| !self.members.contains_key(w))
            || self.canvas.len() != self.columns.len()
            || self
                .columns
                .iter()
                .map(|c| c.group)
                .collect::<BTreeSet<_>>()
                .len()
                != self.columns.len()
            || self
                .columns
                .iter()
                .any(|c| !self.canvas.contains_key(&c.group))
            || self
                .canvas
                .values()
                .any(|g| !valid_rect(g.rect) || !g.scroll_y.is_finite() || g.scroll_y < 0.0)
        {
            return Err("membership mismatch");
        }
        if self.columns.iter().any(|c| {
            c.windows.is_empty()
                || !c.windows.contains(&c.active)
                || (self.is_minimized(c.active) && c.windows.iter().any(|w| !self.is_minimized(*w)))
        }) {
            return Err("invalid column");
        }
        if self.born_floating.iter().any(|w| !self.is_floating(*w)) {
            return Err("stale born-floating member");
        }
        if self
            .floating
            .iter()
            .any(|(w, r)| !unique.contains(w) || !bounded_rect(*r))
        {
            return Err("invalid floating member");
        }
        if self.fullscreen.iter().any(|w| !unique.contains(w)) {
            return Err("stale fullscreen member");
        }
        if self.check_fullscreen_geometry().is_err() {
            return Err("invalid fullscreen projection");
        }
        if self.maximized.iter().any(|w| !unique.contains(w)) {
            return Err("stale maximized member");
        }
        if !self.maximized.is_empty()
            && [self.strip_camera, self.canvas_camera]
                .iter()
                .any(|camera| maximized_rect(self.viewport, *camera).is_none())
        {
            return Err("invalid maximized viewport");
        }
        if self
            .maximized
            .iter()
            .any(|window| self.maximize_size(self.members[window]).is_err())
        {
            return Err("incompatible maximized member");
        }
        if self.minimized.iter().any(|w| !unique.contains(w)) {
            return Err("stale minimized member");
        }
        if self
            .focus
            .is_some_and(|w| !unique.contains(&w) || self.is_minimized(w))
            || (unique.len() != self.minimized.len() && self.focus.is_none())
        {
            return Err("invalid focus");
        }
        if self
            .history
            .iter()
            .any(|w| !unique.contains(w) || self.is_minimized(*w))
        {
            return Err("stale focus history");
        }
        if self.base_placements().iter().any(|p| {
            !p.rect.is_valid()
                || !p.clip.is_valid()
                || p.rect.width < MIN_SIZE
                || p.rect.height < MIN_SIZE
        }) {
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
            if self.dormant_column(c) {
                continue;
            }
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
                    clip: Rect {
                        x,
                        y: 0.0,
                        width,
                        height: self.viewport.height,
                    },
                    window: *window,
                    rect: Rect {
                        x,
                        y: if c.mode == ColumnMode::Tabs { 0.0 } else { y },
                        width: size.width,
                        height: size.height,
                    },
                    visible: !self.is_minimized(*window)
                        && (c.mode == ColumnMode::Stack || *window == c.active),
                    focused: self.focus == Some(*window),
                });
                y += size.height + self.gap;
            }
            if !self.dormant_column(c) {
                x += width + self.gap;
            }
        }
        result
    }
    fn reveal_focused_canvas_page(&mut self) {
        if self.mode == LayoutMode::Canvas && self.focus.is_some_and(|w| self.is_fullscreen(w)) {
            self.reveal_focus();
        }
    }
    fn reveal_focus(&mut self) {
        // Mutators operate on a private candidate. Do not reveal a candidate
        // whose new page extent will fail its enclosing atomic preflight.
        if (!self.fullscreen.is_empty() || !self.floating.is_empty())
            && self.fullscreen_projection(self.mode).is_err()
        {
            return;
        }
        let Some(window) = self.focus else {
            return;
        };
        if self.mode == LayoutMode::Strip {
            let i = self.column_index(window).unwrap();
            if self.columns[i].mode == ColumnMode::Stack
                && !self.is_fullscreen(window)
                && !self.is_floating(window)
            {
                let rect = self
                    .projected_placements()
                    .into_iter()
                    .find(|p| p.window == window)
                    .unwrap()
                    .rect;
                let shift = reveal_axis(0.0, self.viewport.height, rect.y, rect.height);
                self.columns[i].scroll_y = (self.columns[i].scroll_y + shift).max(0.0);
            }
        }
        if self.mode == LayoutMode::Canvas {
            let i = self.column_index(window).unwrap();
            let group = self.columns[i].group;
            if self.columns[i].mode == ColumnMode::Stack
                && !self.is_fullscreen(window)
                && !self.is_floating(window)
            {
                let rect = self
                    .projected_placements()
                    .into_iter()
                    .find(|p| p.window == window)
                    .unwrap()
                    .rect;
                let container = self.canvas[&group].rect;
                let shift =
                    reveal_axis(container.y, container.height, rect.y, rect.height) - container.y;
                let scroll = self.canvas[&group].scroll_y;
                self.canvas.get_mut(&group).unwrap().scroll_y = (scroll + shift).max(0.0);
            }
        }
        let rect = self
            .projected_placements()
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
        let y = if self.mode == LayoutMode::Strip && !self.is_floating(window) {
            0.0
        } else {
            reveal_axis(
                camera.position.y,
                self.viewport.height / camera.zoom,
                rect.y,
                rect.height,
            )
        };
        let (max_x, max_y) = if self.maximized.is_empty() {
            (MAX_COORD, MAX_COORD)
        } else {
            (
                MAX_COORD - self.viewport.width / camera.zoom,
                MAX_COORD - self.viewport.height / camera.zoom,
            )
        };
        self.camera_mut().position = Point {
            x: x.clamp(-MAX_COORD, max_x),
            y: y.clamp(-MAX_COORD, max_y),
        };
    }
}
// Overlay dimensions are world units: a valid output can become smaller than
// MIN_SIZE under zoom. Base client geometry retains its existing minimum size.
fn bounded_rect(rect: Rect) -> bool {
    valid_rect(rect) && rect.x + rect.width <= MAX_COORD && rect.y + rect.height <= MAX_COORD
}
fn maximized_rect(viewport: Size, camera: Camera) -> Option<Rect> {
    let width = viewport.width / camera.zoom;
    let height = viewport.height / camera.zoom;
    (valid_point(camera.position)
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
        && width <= MAX_COORD
        && height <= MAX_COORD
        && camera.position.x + width <= MAX_COORD
        && camera.position.y + height <= MAX_COORD)
        .then_some(Rect {
            x: camera.position.x,
            y: camera.position.y,
            width,
            height,
        })
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
    #[test]
    fn configured_default_width_only_changes_new_columns_in_both_modes() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.set_column_width(id(0), WidthPolicy::Fixed(420.)).unwrap();
        let group = l.group_for(id(0));
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::Fixed(700.))
            .unwrap();
        assert_eq!(rect(&l, id(0)).width, 420.);
        assert_eq!(l.group_for(id(0)), group);
        assert_eq!(l.focused(), Some(id(0)));
        l.insert(id(1), SizeConstraints::default()).unwrap();
        assert_eq!(rect(&l, id(1)).width, 700.);
        l.reconfigure_with_default_width(
            8.,
            LayoutMode::Strip,
            WidthPolicy::ViewportFraction(0.25),
        )
        .unwrap();
        l.insert(id(2), SizeConstraints::default()).unwrap();
        assert_eq!(rect(&l, id(2)).width, 250.);
        l.set_viewport(Size {
            width: 2000.,
            height: 600.,
        })
        .unwrap();
        assert_eq!(rect(&l, id(0)).width, 420.);
        assert_eq!(rect(&l, id(1)).width, 700.);
        assert_eq!(rect(&l, id(2)).width, 500.);
        l.set_mode(LayoutMode::Canvas);
        l.insert(id(3), SizeConstraints::default()).unwrap();
        assert_eq!(rect(&l, id(3)).width, 500.);
        assert_eq!(rect(&l, id(0)).width, 420.);
        assert_eq!(rect(&l, id(1)).width, 700.);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn invalid_default_or_future_admission_preserves_the_complete_model() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        let before = format!("{l:?}");
        for policy in [
            WidthPolicy::Fixed(f64::NAN),
            WidthPolicy::Fixed(f64::INFINITY),
            WidthPolicy::Fixed(0.),
            WidthPolicy::Fixed(MAX_COORD + 1.),
            WidthPolicy::ViewportFraction(0.),
            WidthPolicy::ViewportFraction(MAX_COORD),
        ] {
            assert_eq!(
                l.reconfigure_with_default_width(20., LayoutMode::Canvas, policy),
                Err(LayoutError::InvalidGeometry)
            );
            assert_eq!(format!("{l:?}"), before);
        }
        l.reconfigure_with_default_width(
            8.,
            LayoutMode::Strip,
            WidthPolicy::Fixed(MAX_COORD - 100.),
        )
        .unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.insert(id(1), SizeConstraints::default()),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::ViewportFraction(16.))
            .unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.set_viewport(Size {
                width: MAX_COORD,
                height: 600.
            }),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
    }
    #[test]
    fn rejected_and_remapped_admissions_use_the_current_default_without_stealing_focus() {
        let mut l = layout();
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::Fixed(760.))
            .unwrap();
        l.insert(
            id(0),
            SizeConstraints {
                min: Size {
                    width: MAX_COORD - 600.,
                    height: 1.,
                },
                max: None,
            },
        )
        .unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.insert_with_focus(id(1), SizeConstraints::default(), false),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::Fixed(400.))
            .unwrap();
        l.insert_with_focus(id(1), SizeConstraints::default(), false)
            .unwrap();
        assert_eq!(l.focused(), Some(id(0)));
        assert_eq!(rect(&l, id(1)).width, 400.);
        l.remove(id(1)).unwrap();
        l.reconfigure_with_default_width(
            8.,
            LayoutMode::Strip,
            WidthPolicy::ViewportFraction(0.25),
        )
        .unwrap();
        l.insert_with_focus(id(1), SizeConstraints::default(), false)
            .unwrap();
        assert_eq!(rect(&l, id(1)).width, 250.);
        assert_eq!(l.focused(), Some(id(0)));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn configured_width_still_honors_each_client_size_constraints() {
        let mut l = layout();
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::Fixed(200.))
            .unwrap();
        l.insert(
            id(0),
            SizeConstraints {
                min: Size {
                    width: 350.,
                    height: 1.,
                },
                max: Some(Size {
                    width: 450.,
                    height: 600.,
                }),
            },
        )
        .unwrap();
        assert_eq!(rect(&l, id(0)).width, 350.);
        l.reconfigure_with_default_width(8., LayoutMode::Strip, WidthPolicy::Fixed(800.))
            .unwrap();
        l.insert(
            id(1),
            SizeConstraints {
                min: Size {
                    width: 1.,
                    height: 1.,
                },
                max: Some(Size {
                    width: 450.,
                    height: 600.,
                }),
            },
        )
        .unwrap();
        assert_eq!(rect(&l, id(0)).width, 350.);
        assert_eq!(rect(&l, id(1)).width, 450.);
        assert_eq!(l.default_width(), WidthPolicy::Fixed(800.));
        assert_eq!(l.validate(), Ok(()));
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
    // Include future mode transitions: rollback must preserve dormant geometry,
    // cameras and one-time conversion flags as well as the current scene.
    fn assert_same_observations(a: &Layout, b: &Layout) {
        let (mut a, mut b) = (a.clone(), b.clone());
        for mode in [
            a.mode(),
            LayoutMode::Canvas,
            LayoutMode::Strip,
            LayoutMode::Canvas,
        ] {
            a.set_mode(mode);
            b.set_mode(mode);
            assert_eq!(a.gap(), b.gap());
            assert_eq!(a.mode(), b.mode());
            assert_eq!(a.len(), b.len());
            assert_eq!(a.focused(), b.focused());
            assert_eq!(a.viewport(), b.viewport());
            assert_eq!(a.camera(), b.camera());
            assert_eq!(a.placements(), b.placements());
            assert_eq!(a.visible_placements(), b.visible_placements());
            assert_eq!(a.canvas_groups(), b.canvas_groups());
        }
    }
    #[test]
    fn reconfigure_changes_gap_preserving_members_focus_and_width_policies() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_column_width(id(0), WidthPolicy::Fixed(123.0))
            .unwrap();
        l.set_column_width(id(1), WidthPolicy::ViewportFraction(0.25))
            .unwrap();
        l.focus_window(id(1)).unwrap();
        let groups: Vec<_> = (0..3).map(|n| l.group_for(id(n))).collect();
        let before = l.placements();
        l.reconfigure(27.0, LayoutMode::Strip).unwrap();
        let after = l.placements();
        assert_eq!(l.gap(), 27.0);
        assert_eq!(l.focused(), Some(id(1)));
        assert_eq!(l.len(), 3);
        for n in 0..3 {
            assert_eq!(l.group_for(id(n)), groups[n as usize]);
            assert_eq!(before[n as usize].window, after[n as usize].window);
            assert_eq!(before[n as usize].rect.width, after[n as usize].rect.width);
        }
        assert_eq!(after[1].rect.x - before[1].rect.x, 19.0);
        assert_eq!(after[2].rect.x - before[2].rect.x, 38.0);
        let viewport = l.viewport();
        l.set_viewport(Size {
            width: viewport.width * 2.0,
            ..viewport
        })
        .unwrap();
        assert_eq!(rect(&l, id(0)).width, 123.0);
        assert_eq!(rect(&l, id(1)).width, before[1].rect.width * 2.0);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn reconfigure_modes_retain_saved_groups_tabs_and_cameras() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        let group = l.group_for(id(0));
        l.reconfigure(16.0, LayoutMode::Canvas).unwrap();
        l.pan(-40.0, 25.0).unwrap();
        let camera = l.camera();
        let groups = l.canvas_groups();
        let focus = l.focused();
        l.reconfigure(16.0, LayoutMode::Strip).unwrap();
        l.reconfigure(16.0, LayoutMode::Canvas).unwrap();
        assert_eq!(l.camera(), camera);
        assert_eq!(l.canvas_groups(), groups);
        assert_eq!(l.focused(), focus);
        assert_eq!(l.group_for(id(1)), group);
        assert_eq!(l.placements().iter().filter(|p| p.visible).count(), 2);
        let saved = l.clone();
        l.reconfigure(16.0, LayoutMode::Canvas).unwrap();
        assert_same_observations(&l, &saved);
    }
    #[test]
    fn reconfigure_rejects_invalid_and_aggregate_geometry_atomically() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        for gap in [f64::NAN, f64::INFINITY, -1.0, 1025.0] {
            let saved = l.clone();
            assert_eq!(
                l.reconfigure(gap, LayoutMode::Canvas),
                Err(LayoutError::InvalidGeometry)
            );
            assert_same_observations(&l, &saved);
        }
        l.set_column_width(id(0), WidthPolicy::Fixed(MAX_COORD - 2048.0))
            .unwrap();
        l.set_column_width(id(1), WidthPolicy::Fixed(1024.0))
            .unwrap();
        let saved = l.clone();
        assert_eq!(
            l.reconfigure(1024.0, LayoutMode::Canvas),
            Err(LayoutError::InvalidGeometry)
        );
        assert_same_observations(&l, &saved);
    }
    #[test]
    fn reconfigure_validates_dormant_stack_geometry_before_publication() {
        let mut l = layout();
        l.set_gap(0.0).unwrap();
        for n in 0..2 {
            l.insert(
                id(n),
                SizeConstraints {
                    min: Size {
                        width: 1.0,
                        height: MAX_COORD,
                    },
                    max: None,
                },
            )
            .unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        l.set_mode(LayoutMode::Canvas);
        // Stack policy is shared. Keep focus on the first member so the second
        // starts exactly at MAX_COORD in each representation before reloading.
        l.focus_window(id(0)).unwrap();
        l.set_column_mode(id(0), ColumnMode::Stack).unwrap();
        let saved = l.clone();
        assert_eq!(
            l.reconfigure(1.0, LayoutMode::Strip),
            Err(LayoutError::InvalidGeometry)
        );
        assert_same_observations(&l, &saved);
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
    fn first_canvas_conversion_uses_current_columns_not_insertion_snapshot() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_column_width(id(0), WidthPolicy::Fixed(700.0))
            .unwrap();
        l.move_focused(Direction::Left).unwrap();
        let expected = l.placements();
        l.set_mode(LayoutMode::Canvas);
        for p in l.placements() {
            assert_eq!(
                p.rect,
                expected.iter().find(|v| v.window == p.window).unwrap().rect
            );
        }
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn first_return_sorts_xy_stable_groups_then_preserves_independent_states() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        for (n, x, y) in [(0, 100.0, 50.0), (1, -100.0, 50.0), (2, -100.0, 0.0)] {
            l.set_canvas_rect(
                id(n),
                Rect {
                    x,
                    y,
                    width: 500.0,
                    height: 600.0,
                },
            )
            .unwrap();
        }
        let canvas = l.placements();
        l.set_mode(LayoutMode::Strip);
        assert_eq!(
            l.placements().iter().map(|p| p.window).collect::<Vec<_>>(),
            vec![id(2), id(1), id(0)]
        );
        l.pan(21.0, 0.0).unwrap();
        let strip_camera = l.camera();
        let strip = l.placements();
        l.set_mode(LayoutMode::Canvas);
        // Scene order follows canonical groups; world rectangles remain independent.
        for p in l.placements() {
            assert_eq!(
                p.rect,
                canvas.iter().find(|v| v.window == p.window).unwrap().rect
            );
        }
        l.pan(-80.0, 70.0).unwrap();
        let canvas_camera = l.camera();
        l.set_canvas_rect(
            id(0),
            Rect {
                x: -1000.0,
                y: 0.0,
                width: 500.0,
                height: 600.0,
            },
        )
        .unwrap();
        l.set_mode(LayoutMode::Strip);
        assert_eq!(l.placements(), strip);
        assert_eq!(l.camera(), strip_camera);
        l.set_mode(LayoutMode::Canvas);
        assert_eq!(l.camera(), canvas_camera);
    }
    #[test]
    fn stable_groups_preserve_tabs_membership_and_destroy_only_when_empty() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        let target = l.group_for(id(0)).unwrap();
        let removed = l.group_for(id(1)).unwrap();
        l.join_column(id(1), id(0)).unwrap();
        assert_eq!(l.group_for(id(1)), Some(target));
        assert_eq!(l.canvas_groups().len(), 2);
        assert_eq!(
            l.set_canvas_group_rect(
                removed,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0
                }
            ),
            Err(LayoutError::UnknownGroup)
        );
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        l.focus_window(id(0)).unwrap();
        l.set_mode(LayoutMode::Canvas);
        assert!(
            !l.placements()
                .iter()
                .find(|p| p.window == id(1))
                .unwrap()
                .visible
        );
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), None);
        assert_eq!(l.cycle_tab(true).unwrap(), Some(id(1)));
        l.set_canvas_rect(
            id(1),
            Rect {
                x: -500.0,
                y: 200.0,
                width: 400.0,
                height: 300.0,
            },
        )
        .unwrap();
        assert_eq!(rect(&l, id(0)).x, -500.0);
        l.remove(id(0)).unwrap();
        assert_eq!(l.group_for(id(1)), Some(target));
        l.remove(id(1)).unwrap();
        assert!(l.canvas_groups().iter().all(|g| g.id != target));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn directional_scoring_uses_projected_edge_distance_after_angle() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        for (n, x, width) in [(0, 0.0, 100.0), (1, 200.0, 100.0), (2, 150.0, 1000.0)] {
            l.set_canvas_rect(
                id(n),
                Rect {
                    x,
                    y: 0.0,
                    width,
                    height: 100.0,
                },
            )
            .unwrap();
        }
        l.focus_window(id(0)).unwrap();
        // Candidate 2 has a farther center but a nearer leading edge.
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(2)));
    }
    #[test]
    fn xdg_constraints_handle_each_unlimited_axis_and_reject_conflicts() {
        let c = SizeConstraints::from_xdg((0, 20), (0, 500)).unwrap();
        assert_eq!(
            c.min,
            Size {
                width: 1.0,
                height: 20.0
            }
        );
        assert_eq!(
            c.max.unwrap(),
            Size {
                width: MAX_COORD,
                height: 500.0
            }
        );
        assert_eq!(
            SizeConstraints::from_xdg((0, 0), (300, 0))
                .unwrap()
                .max
                .unwrap()
                .height,
            MAX_COORD
        );
        assert_eq!(
            SizeConstraints::from_xdg((0, 0), (i32::MAX, i32::MAX))
                .unwrap()
                .max
                .unwrap()
                .width,
            MAX_COORD
        );
        assert!(SizeConstraints::from_xdg((40, 0), (30, 0)).is_err());
        assert!(SizeConstraints::from_xdg((-1, 0), (0, 0)).is_err());
    }
    #[test]
    fn equal_canvas_positions_sort_by_stable_group_identity() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.move_focused(Direction::Left).unwrap();
        l.move_focused(Direction::Left).unwrap();
        l.set_mode(LayoutMode::Canvas);
        for n in 0..3 {
            l.set_canvas_rect(
                id(n),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 500.0,
                    height: 600.0,
                },
            )
            .unwrap();
        }
        l.set_mode(LayoutMode::Strip);
        assert_eq!(
            l.placements().iter().map(|p| p.window).collect::<Vec<_>>(),
            vec![id(0), id(1), id(2)]
        );
    }
    #[test]
    fn canvas_stack_focus_scrolls_local_content_and_picking_obeys_group_clip() {
        let mut l = layout();
        let c = SizeConstraints {
            min: Size {
                width: 1.0,
                height: 250.0,
            },
            max: None,
        };
        for n in 0..3 {
            l.insert(id(n), c).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.join_column(id(2), id(0)).unwrap();
        l.set_mode(LayoutMode::Canvas);
        l.set_canvas_rect(
            id(0),
            Rect {
                x: 100.0,
                y: 100.0,
                width: 500.0,
                height: 300.0,
            },
        )
        .unwrap();
        l.focus_window(id(2)).unwrap();
        assert_eq!(rect(&l, id(2)).y + rect(&l, id(2)).height, 400.0);
        assert_eq!(l.hit_test(Point { x: 110.0, y: 90.0 }), None);
        assert_eq!(l.hit_test(Point { x: 110.0, y: 200.0 }), Some(id(2)));
        l.focus_window(id(0)).unwrap();
        assert_eq!(rect(&l, id(0)).y, 100.0);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn minimized_stack_members_reserve_geometry_and_navigation_skips_them() {
        let mut l = layout();
        for n in 0..4 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.join_column(id(2), id(0)).unwrap();
        l.focus_window(id(0)).unwrap();
        let geometry = l
            .placements()
            .iter()
            .map(|p| (p.window, p.rect))
            .collect::<BTreeMap<_, _>>();
        let camera = l.camera();
        l.set_minimized(id(1), true).unwrap();
        assert_eq!(l.camera(), camera);
        assert_eq!(l.len(), 4);
        for p in l.placements() {
            assert_eq!(p.rect, geometry[&p.window]);
        }
        assert!(
            !l.placements()
                .iter()
                .find(|p| p.window == id(1))
                .unwrap()
                .visible
        );
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), Some(id(2)));
        assert_eq!(l.cycle_tab(true).unwrap(), Some(id(0)));
        assert_eq!(l.cycle_tab(false).unwrap(), Some(id(2)));
        let before = l.clone();
        assert_eq!(l.focus_window(id(1)), Err(LayoutError::MinimizedWindow));
        assert_eq!(
            l.join_column(id(1), id(3)),
            Err(LayoutError::MinimizedWindow)
        );
        assert_same_observations(&l, &before);
        l.focus_window(id(3)).unwrap();
        l.set_minimized(id(0), true).unwrap();
        l.set_minimized(id(2), true).unwrap();
        assert_eq!(l.focus_direction(Direction::Left).unwrap(), None);
        l.set_minimized(id(3), true).unwrap();
        assert_eq!(l.focused(), None);
        assert!(l.canvas_groups().is_empty());
        assert!(l.visible_placements().is_empty());
        assert!(l.placements().iter().all(|p| !p.visible && !p.focused));
        assert_eq!(l.cycle_tab(true).unwrap(), None);
        l.move_focused(Direction::Right).unwrap();
        assert_eq!(l.validate(), Ok(()));
        l.set_minimized(id(1), false).unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        assert!(!l.visible_placements().is_empty());
        l.insert(id(4), SizeConstraints::default()).unwrap();
        l.set_minimized(id(1), false).unwrap();
        assert_eq!(
            l.focused(),
            Some(id(4)),
            "idempotent restore must not steal focus"
        );
    }

    #[test]
    fn minimize_tab_updates_active_and_remove_never_focuses_hidden_history() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        l.focus_window(id(0)).unwrap();
        l.focus_window(id(1)).unwrap();
        l.set_minimized(id(1), true).unwrap();
        assert_eq!(l.focused(), Some(id(0)));
        assert_eq!(
            l.canvas_groups()
                .iter()
                .find(|g| g.id == l.group_for(id(0)).unwrap())
                .unwrap()
                .active,
            id(0)
        );
        l.remove(id(0)).unwrap();
        assert_eq!(l.focused(), Some(id(2)));
        l.remove(id(2)).unwrap();
        assert_eq!(l.focused(), None);
        l.remove(id(1)).unwrap();
        assert!(!l.is_minimized(id(1)));
        l.insert(id(1), SizeConstraints::default()).unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        assert_eq!(l.validate(), Ok(()));
        let before = l.clone();
        assert_eq!(
            l.set_minimized(id(99), true),
            Err(LayoutError::UnknownWindow)
        );
        assert_same_observations(&l, &before);
    }

    #[test]
    fn minimized_members_keep_constraints_and_saved_modes() {
        let mut l = layout();
        let constraints = SizeConstraints::from_xdg((310, 210), (450, 350)).unwrap();
        l.insert(id(0), constraints).unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        l.set_column_width(id(0), WidthPolicy::Fixed(400.0))
            .unwrap();
        l.set_mode(LayoutMode::Canvas);
        l.set_canvas_rect(
            id(0),
            Rect {
                x: -800.0,
                y: 120.0,
                width: 420.0,
                height: 320.0,
            },
        )
        .unwrap();
        let group = l.group_for(id(0)).unwrap();
        let saved = l.canvas[&group];
        let columns = l
            .columns
            .iter()
            .map(|c| (c.group, c.width))
            .collect::<Vec<_>>();
        l.set_minimized(id(0), true).unwrap();
        l.set_mode(LayoutMode::Strip);
        l.set_mode(LayoutMode::Canvas);
        assert_eq!(l.canvas[&group].rect, saved.rect);
        assert_eq!(l.canvas[&group].scroll_y, saved.scroll_y);
        assert_eq!(l.members[&id(0)], constraints);
        for (group, width) in columns {
            assert_eq!(
                l.columns.iter().find(|c| c.group == group).unwrap().width,
                width
            );
        }
        l.set_minimized(id(0), false).unwrap();
        assert_eq!(l.focused(), Some(id(0)));
        let p = l
            .visible_placements()
            .into_iter()
            .find(|p| p.window == id(0))
            .unwrap();
        assert_eq!(p.rect, saved.rect);
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
            match seed % 22 {
                0 if live.len() < 100 => {
                    let w = id(next);
                    next += 1;
                    if seed & 256 == 0 {
                        l.insert(w, SizeConstraints::default()).unwrap();
                    } else {
                        l.insert_floating_with_focus(w, SizeConstraints::default(), None, true)
                            .unwrap();
                    }
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
                        if l.is_minimized(w) {
                            assert_eq!(l.focus_window(w), Err(LayoutError::MinimizedWindow));
                        } else {
                            l.focus_window(w).unwrap();
                        }
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
                        .unwrap_or_else(|e| assert_eq!(e, LayoutError::FloatingWindow));
                    }
                }
                7 => {
                    if let (Some(w), Some(f)) = (target, l.focused()) {
                        if l.is_minimized(w) {
                            assert_eq!(l.join_column(w, f), Err(LayoutError::MinimizedWindow));
                        } else {
                            l.join_column(w, f)
                                .unwrap_or_else(|e| assert_eq!(e, LayoutError::FloatingWindow));
                        }
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
                11 | 12 => {
                    if let Some(w) = target {
                        l.set_minimized(w, seed % 22 == 11).unwrap();
                    }
                }
                13 => {
                    l.cycle_tab(seed & 128 == 0).unwrap();
                }
                14 | 15 => {
                    if let Some(w) = target {
                        let result = l.set_maximized(w, seed % 22 == 14);
                        assert!(matches!(
                            result,
                            Ok(()) | Err(LayoutError::InvalidConstraints)
                        ));
                    }
                }
                16 | 17 => {
                    if let Some(w) = target {
                        assert!(matches!(
                            l.set_fullscreen(w, seed % 22 == 16),
                            Ok(()) | Err(LayoutError::InvalidConstraints)
                        ));
                    }
                }
                18 | 19 => {
                    if let Some(w) = target {
                        l.set_floating(w, seed % 22 == 18, None).unwrap();
                    }
                }
                20 => {
                    if let Some(w) = target {
                        l.set_constraints(
                            w,
                            SizeConstraints {
                                min: Size {
                                    width: 20.0 + (seed % 120) as f64,
                                    height: 20.0,
                                },
                                max: None,
                            },
                        )
                        .unwrap();
                    }
                }
                21 => {
                    if let Some(w) = target
                        && let Some(r) = l.floating_rect(w)
                    {
                        l.set_floating_rect(
                            w,
                            Rect {
                                x: (seed % 3000) as f64 - 1500.0,
                                ..r
                            },
                        )
                        .unwrap();
                    }
                }
                _ => {}
            }
            assert_eq!(l.validate(), Ok(()));
            for p in l.placements() {
                if l.is_maximized(p.window) && !l.is_fullscreen(p.window) {
                    let view = l.camera().rect_to_view(p.rect);
                    assert!(view.x.abs() < 1e-6 && view.y.abs() < 1e-6);
                    assert!((view.width - l.viewport.width).abs() < 1e-6);
                    assert!((view.height - l.viewport.height).abs() < 1e-6);
                }
                if l.is_fullscreen(p.window) {
                    assert_eq!(p.rect, p.clip);
                    assert_eq!(p.rect.width, l.viewport.width / l.camera().zoom);
                    assert_eq!(p.rect.height, l.viewport.height / l.camera().zoom);
                }
                assert_eq!(p.focused, l.focused() == Some(p.window));
                assert!(!l.is_minimized(p.window) || !p.visible);
            }
            if l.mode() == LayoutMode::Canvas {
                let disjoint = |a: Rect, b: Rect| {
                    a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y
                };
                let pages: Vec<_> = l
                    .placements()
                    .into_iter()
                    .filter(|p| l.is_fullscreen(p.window))
                    .collect();
                for (i, page) in pages.iter().enumerate() {
                    for column in l.columns.iter().filter(|c| !l.dormant_column(c)) {
                        assert!(disjoint(page.clip, l.canvas[&column.group].rect));
                    }
                    for other in &pages[i + 1..] {
                        assert!(disjoint(page.clip, other.clip));
                    }
                }
            }
            assert_eq!(
                l.placements()
                    .into_iter()
                    .map(|p| p.window)
                    .collect::<BTreeSet<_>>(),
                live
            );
        }
    }
    #[test]
    fn maximize_preserves_base_geometry_focus_constraints_and_minimization() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.join_column(id(1), id(0)).unwrap();
        l.focus_window(id(0)).unwrap();
        let base = l.base_placements();
        let camera = l.camera();
        l.set_maximized(id(1), true).unwrap();
        l.set_maximized(id(1), true).unwrap();
        assert_eq!(l.focused(), Some(id(0)));
        assert_eq!(l.camera(), camera);
        assert_eq!(l.base_placements(), base);
        let max = l
            .placements()
            .into_iter()
            .find(|p| p.window == id(1))
            .unwrap();
        assert_eq!(
            l.camera().rect_to_view(max.rect),
            Rect {
                x: 0.,
                y: 0.,
                width: 1000.,
                height: 600.
            }
        );
        assert_eq!(max.rect, max.clip);
        l.set_maximized(id(1), false).unwrap();
        assert_eq!(l.placements(), base);
        l.set_maximized(id(1), true).unwrap();
        l.set_minimized(id(1), true).unwrap();
        assert!(l.is_maximized(id(1)));
        assert!(
            !l.placements()
                .into_iter()
                .find(|p| p.window == id(1))
                .unwrap()
                .visible
        );
        l.set_minimized(id(1), false).unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        assert!(l.is_maximized(id(1)));
        l.set_column_mode(id(1), ColumnMode::Tabs).unwrap();
        l.focus_window(id(0)).unwrap();
        assert!(
            !l.placements()
                .into_iter()
                .find(|p| p.window == id(1))
                .unwrap()
                .visible
        );
        l.cycle_tab(true).unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        l.set_maximized(id(1), false).unwrap();
        assert_eq!(l.placements(), l.base_placements());
        l.set_maximized(id(1), true).unwrap();
        l.remove(id(1)).unwrap();
        assert!(!l.is_maximized(id(1)));
        assert_eq!(
            l.set_maximized(id(1), true),
            Err(LayoutError::UnknownWindow)
        );
        l.insert(id(1), SizeConstraints::default()).unwrap();
        assert!(!l.is_maximized(id(1)));
        assert_eq!(l.validate(), Ok(()));
    }

    #[test]
    fn maximized_canvas_navigation_uses_saved_geometry_across_camera_and_modes() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
            l.set_maximized(id(n), true).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.focus_direction(Direction::Right), Ok(Some(id(1))));
        assert_eq!(l.focus_direction(Direction::Right), Ok(Some(id(2))));
        assert_eq!(l.focus_direction(Direction::Left), Ok(Some(id(1))));
        let base = l.base_placements();
        l.set_viewport(Size {
            width: 800.,
            height: 400.,
        })
        .unwrap();
        l.set_camera(Camera::new(Point { x: 100., y: -50. }, 2.).unwrap())
            .unwrap();
        for placement in l.placements() {
            assert_eq!(
                l.camera().rect_to_view(placement.rect),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 800.,
                    height: 400.
                }
            );
        }
        assert_eq!(l.base_placements(), base);
        l.set_mode(LayoutMode::Strip);
        l.set_mode(LayoutMode::Canvas);
        for n in 0..3 {
            l.set_maximized(id(n), false).unwrap();
        }
        assert_eq!(l.placements(), base);
        assert_eq!(l.validate(), Ok(()));
    }

    #[test]
    fn maximize_bounds_reject_atomically_and_reveal_stays_safe() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.set_camera(
            Camera::new(
                Point {
                    x: MAX_COORD,
                    y: 0.,
                },
                1.,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            l.set_maximized(id(0), true),
            Err(LayoutError::InvalidGeometry)
        );
        assert!(!l.is_maximized(id(0)));
        l.set_camera(Camera::default()).unwrap();
        l.set_maximized(id(0), true).unwrap();
        let before = l.camera();
        assert_eq!(
            l.set_camera(
                Camera::new(
                    Point {
                        x: MAX_COORD,
                        y: 0.
                    },
                    1.
                )
                .unwrap()
            ),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.camera(), before);
        l.set_mode(LayoutMode::Canvas);
        l.set_canvas_rect(
            id(0),
            Rect {
                x: MAX_COORD,
                y: MAX_COORD,
                width: 500.,
                height: 600.,
            },
        )
        .unwrap();
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.validate(), Ok(()));
        let before = l.camera();
        assert_eq!(
            l.rebase_canvas(Point { x: -1000., y: 0. }),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.camera(), before);
        assert_eq!(
            l.set_viewport(Size {
                width: MAX_COORD,
                height: 600.
            }),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.viewport().width, 1000.);
        l.set_maximized(id(0), false).unwrap();
        l.set_camera(Camera::new(Point::default(), 0.01).unwrap())
            .unwrap();
        l.set_viewport(Size {
            width: MAX_COORD,
            height: 600.,
        })
        .unwrap();
        assert_eq!(
            l.set_maximized(id(0), true),
            Err(LayoutError::InvalidGeometry)
        );
    }
    #[test]
    fn maximize_declines_incompatible_bounds_and_clears_after_size_changes() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        let fixed = SizeConstraints::from_xdg((400, 300), (400, 300)).unwrap();
        assert_eq!(l.maximize_size(fixed), Err(LayoutError::InvalidConstraints));
        l.set_constraints(id(0), fixed).unwrap();
        let base = l.placements();
        assert_eq!(
            l.set_maximized(id(0), true),
            Err(LayoutError::InvalidConstraints)
        );
        assert_eq!(l.placements(), base);
        assert!(!l.is_maximized(id(0)));
        let oversize = SizeConstraints::from_xdg((1001, 1), (0, 0)).unwrap();
        assert_eq!(
            l.maximize_size(oversize),
            Err(LayoutError::InvalidConstraints)
        );
        let compatible = SizeConstraints::from_xdg((100, 100), (1000, 600)).unwrap();
        assert_eq!(l.maximize_size(compatible), Ok(l.viewport()));
        l.set_constraints(id(0), compatible).unwrap();
        l.set_maximized(id(0), true).unwrap();
        l.set_constraints(id(0), fixed).unwrap();
        assert!(!l.is_maximized(id(0)));
        assert_eq!(l.placements(), l.base_placements());
        l.set_constraints(id(0), compatible).unwrap();
        l.set_maximized(id(0), true).unwrap();
        l.set_viewport(Size {
            width: 1200.,
            height: 600.,
        })
        .unwrap();
        assert!(!l.is_maximized(id(0)));
        assert_eq!(l.placements(), l.base_placements());
        l.set_viewport(Size {
            width: 1000.,
            height: 600.,
        })
        .unwrap();
        l.set_mode(LayoutMode::Canvas);
        l.set_maximized(id(0), true).unwrap();
        l.set_camera(Camera::new(Point::default(), 0.5).unwrap())
            .unwrap();
        assert!(!l.is_maximized(id(0)));
        l.set_camera(Camera::new(Point::default(), 2.).unwrap())
            .unwrap();
        let minimum = SizeConstraints::from_xdg((1, 1), (700, 500)).unwrap();
        l.set_constraints(id(0), minimum).unwrap();
        l.set_maximized(id(0), true).unwrap();
        l.set_mode(LayoutMode::Strip);
        assert!(!l.is_maximized(id(0)));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn fullscreen_twenty_window_pages_keep_order_widths_focus_and_restoration() {
        let mut l = layout();
        for n in 0..20 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        let base = l.base_placements();
        let columns = l
            .columns
            .iter()
            .map(|c| (c.group, c.width))
            .collect::<Vec<_>>();
        let focused = l.focused();
        l.set_fullscreen(id(7), true).unwrap();
        let projected = l.placements();
        assert_eq!(projected.len(), 20);
        assert_eq!(
            projected.iter().map(|p| p.window).collect::<Vec<_>>(),
            base.iter().map(|p| p.window).collect::<Vec<_>>()
        );
        let full = projected.iter().find(|p| p.window == id(7)).unwrap();
        let saved = base.iter().find(|p| p.window == id(7)).unwrap();
        assert_eq!(full.rect.x, saved.rect.x + saved.clip.width + l.gap());
        assert_eq!(full.rect.width, l.viewport.width);
        assert_eq!(full.rect, full.clip);
        for before in &base {
            let after = projected
                .iter()
                .find(|p| p.window == before.window)
                .unwrap();
            if before.window != id(7) {
                assert_eq!(before.rect.width, after.rect.width);
            }
            let shift = if before.rect.x > saved.rect.x {
                l.viewport.width + l.gap()
            } else {
                0.0
            };
            if before.window != id(7) {
                assert_eq!(after.rect.x, before.rect.x + shift);
            }
        }
        assert_eq!(l.focused(), focused);
        l.focus_window(id(7)).unwrap();
        assert_eq!(
            l.camera().world_to_view(Point {
                x: full.rect.x,
                y: full.rect.y
            }),
            Point::default()
        );
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(8)));
        assert!(l.is_fullscreen(id(7)));
        assert_eq!(
            l.placements()
                .iter()
                .find(|p| p.window == id(7))
                .unwrap()
                .rect,
            full.rect
        );
        l.set_minimized(id(7), true).unwrap();
        assert!(
            !l.placements()
                .iter()
                .find(|p| p.window == id(7))
                .unwrap()
                .visible
        );
        assert!(l.is_fullscreen(id(7)));
        l.set_minimized(id(7), false).unwrap();
        assert_eq!(l.focused(), Some(id(7)));
        l.set_fullscreen(id(7), false).unwrap();
        assert_eq!(
            l.base_placements()
                .iter()
                .map(|p| p.rect)
                .collect::<Vec<_>>(),
            base.iter().map(|p| p.rect).collect::<Vec<_>>()
        );
        assert_eq!(
            l.columns
                .iter()
                .map(|c| (c.group, c.width))
                .collect::<Vec<_>>(),
            columns
        );
        assert_eq!(l.validate(), Ok(()));
    }

    #[test]
    fn fullscreen_stack_and_inactive_tabs_get_separate_pages_and_rejoin() {
        for mode in [ColumnMode::Stack, ColumnMode::Tabs] {
            let mut l = layout();
            for n in 0..3 {
                l.insert(id(n), SizeConstraints::default()).unwrap();
            }
            l.join_column(id(1), id(0)).unwrap();
            l.join_column(id(2), id(0)).unwrap();
            l.set_column_mode(id(0), mode).unwrap();
            l.focus_window(id(0)).unwrap();
            let base = l.base_placements();
            l.set_fullscreen(id(1), true).unwrap();
            l.set_fullscreen(id(2), true).unwrap();
            let placements = l.placements();
            assert_eq!(placements.iter().filter(|p| p.visible).count(), 3);
            let ordinary = placements.iter().find(|p| p.window == id(0)).unwrap();
            assert_eq!(ordinary.rect.height, l.viewport.height);
            let first = placements.iter().find(|p| p.window == id(1)).unwrap();
            let second = placements.iter().find(|p| p.window == id(2)).unwrap();
            assert_eq!(second.rect.x, first.rect.x + first.rect.width + l.gap());
            assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(1)));
            assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(2)));
            l.focus_window(id(0)).unwrap();
            l.set_fullscreen(id(1), false).unwrap();
            l.set_fullscreen(id(2), false).unwrap();
            assert_eq!(l.placements(), base);
            assert_eq!(l.validate(), Ok(()));
        }
    }

    #[test]
    fn fullscreen_precedes_maximize_and_canvas_pages_follow_saved_groups() {
        let mut l = layout();
        for n in 0..2 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        l.set_camera(
            Camera::new(
                Point {
                    x: -100.0,
                    y: -200.0,
                },
                2.0,
            )
            .unwrap(),
        )
        .unwrap();
        l.set_maximized(id(0), true).unwrap();
        let group = l.group_for(id(0)).unwrap();
        let base = l.canvas[&group].rect;
        l.set_fullscreen(id(0), true).unwrap();
        let page = l
            .placements()
            .into_iter()
            .find(|p| p.window == id(0))
            .unwrap();
        let saved_right = l
            .canvas
            .values()
            .map(|g| g.rect.x + g.rect.width)
            .reduce(f64::max)
            .unwrap();
        assert_eq!(page.rect.x, saved_right + l.gap());
        assert_eq!(page.rect.y, base.y);
        assert_eq!(page.rect.width, l.viewport.width / 2.0);
        assert_eq!(page.rect.height, l.viewport.height / 2.0);
        assert_eq!(page.rect, page.clip);
        l.pan(123.0, -84.0).unwrap();
        assert_eq!(
            l.placements()
                .into_iter()
                .find(|p| p.window == id(0))
                .unwrap()
                .rect,
            page.rect
        );
        assert!(l.is_maximized(id(0)));
        l.set_fullscreen(id(0), false).unwrap();
        let restored = l
            .placements()
            .into_iter()
            .find(|p| p.window == id(0))
            .unwrap();
        assert_eq!(
            restored.rect,
            maximized_rect(l.viewport, l.camera()).unwrap()
        );
        assert_eq!(l.canvas[&group].rect, base);
        l.set_fullscreen(id(0), true).unwrap();
        l.remove(id(0)).unwrap();
        assert!(!l.is_fullscreen(id(0)));
        assert_eq!(
            l.set_fullscreen(id(0), true),
            Err(LayoutError::UnknownWindow)
        );
    }

    #[test]
    fn fullscreen_constraints_resize_and_overflow_are_explicit_and_atomic() {
        let mut l = layout();
        let small = SizeConstraints::from_xdg((10, 10), (100, 100)).unwrap();
        l.insert(id(0), small).unwrap();
        assert_eq!(l.fullscreen_size(small), Ok(l.viewport));
        l.set_fullscreen(id(0), true).unwrap();
        l.set_constraints(
            id(0),
            SizeConstraints::from_xdg((500, 500), (0, 0)).unwrap(),
        )
        .unwrap();
        l.set_viewport(Size {
            width: 400.0,
            height: 400.0,
        })
        .unwrap();
        assert!(!l.is_fullscreen(id(0)));
        assert_eq!(
            l.set_fullscreen(id(0), true),
            Err(LayoutError::InvalidConstraints)
        );
        l.set_constraints(id(0), SizeConstraints::default())
            .unwrap();
        l.set_fullscreen(id(0), true).unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.set_column_width(id(0), WidthPolicy::Fixed(MAX_COORD - 100.0)),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
        l.set_mode(LayoutMode::Canvas);
        let group = l.group_for(id(0)).unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.set_canvas_group_rect(
                group,
                Rect {
                    x: MAX_COORD - 500.0,
                    y: 0.0,
                    width: 300.0,
                    height: 400.0
                }
            ),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
        l.set_camera(Camera::new(Point::default(), 100.0).unwrap())
            .unwrap();
        assert!(l.is_fullscreen(id(0)));
        l.set_constraints(
            id(0),
            SizeConstraints::from_xdg((100, 100), (0, 0)).unwrap(),
        )
        .unwrap();
        assert!(!l.is_fullscreen(id(0)));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn fullscreen_virtual_strip_columns_keep_adjacent_focus_and_local_vertical_navigation() {
        let mut l = layout();
        for n in 0..5 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.join_column(id(2), id(1)).unwrap();
        l.focus_window(id(1)).unwrap();
        l.set_fullscreen(id(4), true).unwrap();
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.focus_direction(Direction::Up).unwrap(), None);
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), None);
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(1)));
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(3)));
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(4)));
        assert_eq!(l.focus_direction(Direction::Up).unwrap(), None);
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), None);
        assert_eq!(l.focus_direction(Direction::Left).unwrap(), Some(id(3)));
        l.focus_window(id(1)).unwrap();
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), Some(id(2)));
        assert_eq!(l.focus_direction(Direction::Up).unwrap(), Some(id(1)));
        l.set_fullscreen(id(2), true).unwrap();
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), None);
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(2)));
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(3)));
        l.set_minimized(id(2), true).unwrap();
        l.focus_window(id(1)).unwrap();
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(3)));
        assert_eq!(l.validate(), Ok(()));
    }

    #[test]
    fn fullscreen_canvas_bank_reserves_all_saved_groups_and_moves_on_insert() {
        let mut l = layout();
        for n in 0..2 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        let saved = l.canvas.clone();
        l.set_fullscreen(id(0), true).unwrap();
        l.set_fullscreen(id(1), true).unwrap();
        l.focus_window(id(1)).unwrap();
        let before = l
            .placements()
            .into_iter()
            .find(|p| p.window == id(1))
            .unwrap()
            .rect;
        let disjoint = |a: Rect, b: Rect| {
            a.x + a.width <= b.x
                || b.x + b.width <= a.x
                || a.y + a.height <= b.y
                || b.y + b.height <= a.y
        };
        for page in l.placements().iter().filter(|p| l.is_fullscreen(p.window)) {
            for group in l.canvas.values() {
                assert!(disjoint(page.clip, group.rect));
            }
        }
        // Place an ordinary group exactly where the previous adjacent-page
        // policy would have put the focused page. Its saved geometry is kept;
        // all fullscreen pages move to the right of the complete saved extent.
        l.insert_with_focus(id(3), SizeConstraints::default(), false)
            .unwrap();
        assert_eq!(l.focused(), Some(id(1)));
        let pages: Vec<_> = l
            .placements()
            .into_iter()
            .filter(|p| l.is_fullscreen(p.window))
            .collect();
        assert!(pages[1].rect.x > before.x);
        assert!(disjoint(pages[0].clip, pages[1].clip));
        for page in &pages {
            for group in l.canvas.values() {
                assert!(disjoint(page.clip, group.rect));
            }
        }
        let view = l.camera().rect_to_view(pages[1].rect);
        assert_eq!(view.x, 0.0);
        assert_eq!(view.y, 0.0);
        for (group, geometry) in saved {
            assert_eq!(l.canvas[&group].rect, geometry.rect);
        }
        l.set_fullscreen(id(0), false).unwrap();
        l.set_fullscreen(id(1), false).unwrap();
        assert_eq!(l.placements(), l.base_placements());
        assert_eq!(l.validate(), Ok(()));
    }

    #[test]
    fn fullscreen_checks_ordinary_right_edges_after_reserved_pages_atomically() {
        let mut l = Layout::new(Size {
            width: 400.0,
            height: 400.0,
        })
        .unwrap();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        l.set_fullscreen(id(0), true).unwrap();
        let before = format!("{l:?}");
        assert_eq!(
            l.set_column_width(id(1), WidthPolicy::Fixed(MAX_COORD - 600.0)),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(format!("{l:?}"), before);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn fullscreen_canvas_focus_tracks_bank_after_unfocused_remove_and_group_move() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.set_mode(LayoutMode::Canvas);
        l.set_fullscreen(id(0), true).unwrap();
        l.set_fullscreen(id(1), true).unwrap();
        l.focus_window(id(1)).unwrap();
        let focused_page = |layout: &Layout| {
            layout
                .placements()
                .into_iter()
                .find(|p| p.window == id(1))
                .unwrap()
                .rect
        };
        let assert_revealed = |layout: &Layout| {
            assert_eq!(layout.focused(), Some(id(1)));
            assert!(layout.is_fullscreen(id(1)));
            let view = layout.camera().rect_to_view(focused_page(layout));
            assert_eq!((view.x, view.y), (0.0, 0.0));
            assert_eq!(
                (view.width, view.height),
                (layout.viewport.width, layout.viewport.height)
            );
        };
        let old_x = focused_page(&l).x;
        // Closing the rightmost ordinary group moves the entire bank left.
        l.remove(id(2)).unwrap();
        assert!(focused_page(&l).x < old_x);
        assert_revealed(&l);
        let group = l.group_for(id(0)).unwrap();
        let rect = l.canvas[&group].rect;
        let old_x = focused_page(&l).x;
        // Moving an unfocused group's right edge pushes the bank right.
        l.set_canvas_group_rect(group, Rect { x: 7000.0, ..rect })
            .unwrap();
        assert!(focused_page(&l).x > old_x);
        assert_revealed(&l);
        let old_x = focused_page(&l).x;
        // Closing an earlier fullscreen page removes both its saved group and
        // its page, while preserving the later page's focus and visibility.
        l.remove(id(0)).unwrap();
        assert!(focused_page(&l).x < old_x);
        assert_revealed(&l);
        // Explicit camera panning remains independent of this mutation repair.
        l.pan(73.0, 0.0).unwrap();
        assert_eq!(l.camera().rect_to_view(focused_page(&l)).x, -73.0);
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn floating_stack_tabs_restore_and_keep_one_live_placement() {
        for mode in [ColumnMode::Stack, ColumnMode::Tabs] {
            let mut l = layout();
            l.insert(id(0), SizeConstraints::default()).unwrap();
            l.insert(id(1), SizeConstraints::default()).unwrap();
            l.join_column(id(1), id(0)).unwrap();
            l.set_column_mode(id(0), mode).unwrap();
            let saved = l.placements();
            l.set_floating(
                id(1),
                true,
                Some(Rect {
                    x: -120.0,
                    y: 50.0,
                    width: 210.0,
                    height: 180.0,
                }),
            )
            .unwrap();
            let p = l.placements();
            assert_eq!(p.len(), 2);
            assert_eq!(p.iter().filter(|p| p.window == id(1)).count(), 1);
            let ordinary = p.iter().find(|p| p.window == id(0)).unwrap();
            assert!(ordinary.visible);
            assert_eq!(ordinary.rect.height, l.viewport.height);
            assert_eq!(
                l.set_column_width(id(1), WidthPolicy::Fixed(300.0)),
                Err(LayoutError::FloatingWindow)
            );
            // A focused tile cannot cover the separate floating layer.
            l.focus_window(id(0)).unwrap();
            l.set_floating_rect(
                id(1),
                Rect {
                    x: 20.0,
                    y: 20.0,
                    width: 100.0,
                    height: 100.0,
                },
            )
            .unwrap();
            let point = l.camera().world_to_view(Point { x: 30.0, y: 30.0 });
            assert_eq!(l.hit_test(point), Some(id(1)));
            l.focus_window(id(1)).unwrap();
            l.set_floating(id(1), false, None).unwrap();
            assert_eq!(l.placements(), saved);
            assert_eq!(l.validate(), Ok(()));
        }
    }
    #[test]
    fn floating_geometry_survives_overlays_minimize_modes_and_rebases() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        let rect = Rect {
            x: 120.0,
            y: 80.0,
            width: 300.0,
            height: 220.0,
        };
        l.set_floating(id(0), true, Some(rect)).unwrap();
        l.set_maximized(id(0), true).unwrap();
        assert_ne!(l.placements()[0].rect, rect);
        l.set_fullscreen(id(0), true).unwrap();
        l.set_minimized(id(0), true).unwrap();
        assert!(!l.placements()[0].visible);
        assert_eq!(l.floating_rect(id(0)), Some(rect));
        l.set_minimized(id(0), false).unwrap();
        l.set_fullscreen(id(0), false).unwrap();
        assert!(l.is_maximized(id(0)));
        l.set_maximized(id(0), false).unwrap();
        assert_eq!(l.placements()[0].rect, rect);
        l.set_mode(LayoutMode::Canvas);
        assert_eq!(l.floating_rect(id(0)), Some(rect));
        let before = l.camera().rect_to_view(l.placements()[0].rect);
        l.rebase_canvas(Point { x: 75.0, y: 30.0 }).unwrap();
        assert_eq!(l.camera().rect_to_view(l.placements()[0].rect), before);
        let rebased = l.floating_rect(id(0)).unwrap();
        l.set_mode(LayoutMode::Strip);
        assert_eq!(l.floating_rect(id(0)), Some(rebased));
        l.remove(id(0)).unwrap();
        assert_eq!(l.floating_rect(id(0)), None);
    }
    #[test]
    fn floating_constraints_edges_and_motion_are_atomic_and_independent() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.insert(id(1), SizeConstraints::default()).unwrap();
        let sibling = l.placements()[0].rect;
        l.set_floating(
            id(1),
            true,
            Some(Rect {
                x: 40.0,
                y: 60.0,
                width: 300.0,
                height: 200.0,
            }),
        )
        .unwrap();
        l.move_focused(Direction::Down).unwrap();
        l.resize_focused(50.0).unwrap();
        assert_eq!(
            l.floating_rect(id(1)).unwrap(),
            Rect {
                x: 40.0,
                y: 92.0,
                width: 350.0,
                height: 200.0
            }
        );
        assert_eq!(
            l.placements()
                .iter()
                .find(|p| p.window == id(0))
                .unwrap()
                .rect,
            sibling
        );
        l.set_constraints(
            id(1),
            SizeConstraints::from_xdg((100, 110), (200, 150)).unwrap(),
        )
        .unwrap();
        assert_eq!(l.floating_rect(id(1)).unwrap().width, 200.0);
        assert_eq!(l.floating_rect(id(1)).unwrap().height, 150.0);
        let saved = l.floating_rect(id(1)).unwrap();
        assert_eq!(
            l.set_floating_rect(
                id(1),
                Rect {
                    x: MAX_COORD - 1.0,
                    ..saved
                }
            ),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.floating_rect(id(1)), Some(saved));
        assert_eq!(
            l.set_constraints(
                id(1),
                SizeConstraints {
                    min: Size {
                        width: MAX_COORD,
                        height: 110.0
                    },
                    max: None
                }
            ),
            Err(LayoutError::InvalidGeometry)
        );
        assert_eq!(l.floating_rect(id(1)), Some(saved));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn floating_focus_is_reachable_and_movement_keeps_fullscreen_and_tiles_stable() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        let rect = Rect {
            x: 0.0,
            y: -300.0,
            width: 200.0,
            height: 100.0,
        };
        l.set_floating(id(1), true, Some(rect)).unwrap();
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.focus_direction(Direction::Right).unwrap(), Some(id(2)));
        l.focus_window(id(0)).unwrap();
        assert_eq!(l.focus_direction(Direction::Up).unwrap(), Some(id(1)));
        assert_eq!(l.focus_direction(Direction::Down).unwrap(), Some(id(0)));
        l.set_fullscreen(id(0), true).unwrap();
        for mode in [LayoutMode::Strip, LayoutMode::Canvas] {
            l.set_mode(mode);
            l.focus_window(id(1)).unwrap();
            let saved: Vec<_> = l
                .placements()
                .into_iter()
                .filter(|p| p.window != id(1))
                .map(|p| (p.window, p.rect, p.clip))
                .collect();
            l.set_floating_rect(id(1), Rect { x: 5000.0, ..rect })
                .unwrap();
            l.move_focused(Direction::Right).unwrap();
            l.resize_focused(75.0).unwrap();
            let after: Vec<_> = l
                .placements()
                .into_iter()
                .filter(|p| p.window != id(1))
                .map(|p| (p.window, p.rect, p.clip))
                .collect();
            assert_eq!(after, saved);
            assert!(l.is_fullscreen(id(0)));
        }
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn floating_focus_preserves_ordinary_tab_active_and_replacement_metadata() {
        let mut l = layout();
        for n in 0..4 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        for n in 1..4 {
            l.join_column(id(n), id(0)).unwrap();
        }
        l.set_column_mode(id(0), ColumnMode::Tabs).unwrap();
        l.set_floating(id(2), true, None).unwrap();
        l.set_fullscreen(id(3), true).unwrap();
        l.focus_window(id(1)).unwrap();
        l.focus_window(id(2)).unwrap();
        assert_eq!(l.columns[0].active, id(1));
        assert!(
            l.placements()
                .iter()
                .any(|p| p.window == id(1) && p.visible)
        );
        assert!(
            !l.placements()
                .iter()
                .find(|p| p.window == id(0))
                .unwrap()
                .visible
        );
        assert_eq!(l.canvas_groups()[0].active, id(1));
        l.focus_window(id(3)).unwrap();
        assert_eq!(l.columns[0].active, id(1));
        l.set_fullscreen(id(3), false).unwrap();
        assert_eq!(l.columns[0].active, id(3));
        assert!(
            l.placements()
                .iter()
                .any(|p| p.window == id(3) && p.visible)
        );
        l.set_fullscreen(id(3), true).unwrap();
        l.focus_window(id(1)).unwrap();
        l.set_minimized(id(1), true).unwrap();
        assert_eq!(l.columns[0].active, id(0));
        assert_eq!(l.canvas_groups()[0].active, id(0));
        l.set_minimized(id(1), false).unwrap();
        l.remove(id(1)).unwrap();
        assert_eq!(l.columns[0].active, id(0));
        assert_eq!(l.canvas_groups()[0].active, id(0));
        l.remove(id(0)).unwrap();
        assert!(l.canvas_groups().is_empty());
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn born_floating_admission_reserves_no_tile_slot_or_fullscreen_frontier() {
        let mut l = layout();
        for n in 0..3 {
            l.insert(id(n), SizeConstraints::default()).unwrap();
        }
        l.focus_window(id(0)).unwrap();
        l.set_fullscreen(id(0), true).unwrap();
        for mode in [LayoutMode::Strip, LayoutMode::Canvas] {
            l.set_mode(mode);
            let saved: Vec<_> = l
                .placements()
                .iter()
                .map(|p| (p.window, p.rect, p.clip))
                .collect();
            let camera = l.camera();
            let window = id(if mode == LayoutMode::Strip { 10 } else { 11 });
            l.insert_floating_with_focus(
                window,
                SizeConstraints::default(),
                Some(Rect {
                    x: 9000.0,
                    y: 100.0,
                    width: 250.0,
                    height: 180.0,
                }),
                false,
            )
            .unwrap();
            assert_eq!(l.camera(), camera);
            let after: Vec<_> = l
                .placements()
                .iter()
                .filter(|p| p.window != window)
                .map(|p| (p.window, p.rect, p.clip))
                .collect();
            assert_eq!(after, saved);
            assert_eq!(
                l.placements().iter().filter(|p| p.window == window).count(),
                1
            );
            assert_eq!(
                l.join_column(window, id(1)),
                Err(LayoutError::FloatingWindow)
            );
            l.set_floating(window, false, None).unwrap();
            assert!(!l.born_floating.contains(&window));
            assert!(!l.is_floating(window));
            l.remove(window).unwrap();
        }
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn born_floating_materialization_overflow_is_atomic() {
        let mut l = layout();
        l.insert(id(0), SizeConstraints::default()).unwrap();
        l.set_column_width(id(0), WidthPolicy::Fixed(MAX_COORD - l.gap))
            .unwrap();
        l.insert_floating_with_focus(
            id(1),
            SizeConstraints::default(),
            Some(Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }),
            false,
        )
        .unwrap();
        let before = l.placements();
        assert_eq!(
            l.set_floating(id(1), false, None),
            Err(LayoutError::InvalidGeometry)
        );
        assert!(l.is_floating(id(1)));
        assert_eq!(l.placements(), before);
        l.remove(id(1)).unwrap();
        assert!(!l.born_floating.contains(&id(1)));
        assert_eq!(l.validate(), Ok(()));
    }
    #[test]
    fn born_floating_first_unfloat_seeds_normal_canvas_slot_in_either_mode() {
        for materialize_mode in [LayoutMode::Canvas, LayoutMode::Strip] {
            let mut l = layout();
            l.insert(id(0), SizeConstraints::default()).unwrap();
            l.insert(id(1), SizeConstraints::default()).unwrap();
            l.set_mode(LayoutMode::Canvas);
            l.set_canvas_rect(
                id(0),
                Rect {
                    x: 300.0,
                    y: 70.0,
                    width: 500.0,
                    height: 400.0,
                },
            )
            .unwrap();
            l.focus_window(id(0)).unwrap();
            l.insert_floating_with_focus(
                id(2),
                SizeConstraints::default(),
                Some(Rect {
                    x: -1000.0,
                    y: -500.0,
                    width: 180.0,
                    height: 120.0,
                }),
                true,
            )
            .unwrap();
            let neighbors: Vec<_> = [id(0), id(1)]
                .into_iter()
                .map(|w| (w, l.canvas[&l.group_for(w).unwrap()].rect))
                .collect();
            l.set_floating_rect(
                id(2),
                Rect {
                    x: 7000.0,
                    y: 3000.0,
                    width: 270.0,
                    height: 210.0,
                },
            )
            .unwrap();
            l.set_mode(materialize_mode);
            l.set_floating(id(2), false, None).unwrap();
            let expected = Rect {
                x: 808.0,
                y: 70.0,
                width: l.default_width.resolve(l.viewport.width).unwrap(),
                height: l.viewport.height,
            };
            assert_eq!(l.canvas[&l.group_for(id(2)).unwrap()].rect, expected);
            for (window, rect) in neighbors {
                assert_eq!(l.canvas[&l.group_for(window).unwrap()].rect, rect);
            }
            l.set_mode(LayoutMode::Canvas);
            assert_eq!(
                l.placements()
                    .iter()
                    .find(|p| p.window == id(2))
                    .unwrap()
                    .rect,
                expected
            );
            assert_eq!(l.validate(), Ok(()));
        }
    }
}
