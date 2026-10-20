//! One virtual-output composition's geometry and copied surface input regions.
use crate::Fluxwm;
use flux_core::{Point as ModelPoint, Rect, WindowId};
use flux_scene::{SceneInput, SceneSnapshot, round_rect_to_i32};
use smithay::{
    backend::renderer::{
        RendererSuper,
        element::{
            Element, Id, Kind, RenderElement, render_elements,
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::WaylandSurfaceRenderElement,
            utils::CropRenderElement,
        },
        gles::GlesRenderer,
        utils::{CommitCounter, DamageSet, RendererSurfaceStateUserData},
    },
    desktop::PopupManager,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{
        Buffer, IsAlive, Logical, Physical, Point, Rectangle, Scale, Transform,
        user_data::UserDataMap,
    },
    wayland::compositor::{
        RegionAttributes, SurfaceAttributes, TraversalAction, get_parent, with_states,
        with_surface_tree_downward,
    },
};

const MAX_SURFACES: usize = 16_384;
const MAX_REGION_RECTS: usize = 65_536;
const MAX_TREE_DEPTH: usize = 128;

struct Tree {
    window: WindowId,
    popup: bool,
    fullscreen: bool,
    surface: WlSurface,
    origin: Point<i32, Logical>,
    clip: Rectangle<i32, Logical>,
}
struct InputSurface {
    window: WindowId,
    tree_root: WlSurface,
    popup: bool,
    fullscreen: bool,
    surface: WlSurface,
    origin: Point<f64, Logical>,
    bounds: Rectangle<f64, Logical>,
    clip: Rectangle<f64, Logical>,
    region: Option<RegionAttributes>,
}

render_elements! {
    pub SceneRenderElement<=GlesRenderer>;
    Surface=CropRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    FullscreenSurface=FullscreenSurfaceRenderElement,
    Backdrop=SolidColorRenderElement,
}

// Client opaque-region claims are optimization hints, not evidence that ARGB
// pixels are opaque. Fullscreen must always compose its trusted black page
// beneath every client pixel, including popups and subsurfaces. This wrapper
// keeps the surface's actual alpha and draw behavior while exposing no opaque
// regions or direct-scanout storage to the composition machinery.
pub struct FullscreenSurfaceRenderElement(
    CropRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
);

impl Element for FullscreenSurfaceRenderElement {
    fn id(&self) -> &Id {
        self.0.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.0.current_commit()
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        self.0.src()
    }
    fn transform(&self) -> Transform {
        self.0.transform()
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.0.geometry(scale)
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.0.damage_since(scale, commit)
    }
    fn alpha(&self) -> f32 {
        self.0.alpha()
    }
    fn kind(&self) -> Kind {
        self.0.kind()
    }
    // Element::opaque_regions defaults to empty; do not delegate client claims.
}

impl RenderElement<GlesRenderer> for FullscreenSurfaceRenderElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as RendererSuper>::Error> {
        self.0.draw(frame, src, dst, damage, &[], cache)
    }
}

enum Layer {
    Surface(usize),
    Backdrop {
        clip: Rectangle<i32, Logical>,
        element: SolidColorRenderElement,
    },
}

pub struct RuntimeScene {
    geometry: SceneSnapshot,
    // Both lists are front to back, matching Smithay's element ordering.
    input: Vec<InputSurface>,
    layers: Vec<Layer>,
    popup_surfaces: usize,
}

impl RuntimeScene {
    pub fn surface_counts(&self) -> (usize, usize) {
        (self.input.len(), self.popup_surfaces)
    }
    pub fn capture(state: &Fluxwm, viewport: Rect) -> Result<Self, String> {
        let placements = state.layout.placements();
        let mut inputs = Vec::new();
        let mut owners = Vec::new();
        for (z, window) in state.space.elements().enumerate() {
            let Some(top) = window.toplevel() else {
                continue;
            };
            let Some(id) = state.window_id(top.wl_surface()) else {
                continue;
            };
            if !state.windows.get(id).is_some_and(|item| item.mapped) {
                continue;
            }
            let Some(placement) = placements.iter().find(|p| p.window == id && p.visible) else {
                continue;
            };
            let Some(location) = state.space.element_location(window) else {
                continue;
            };
            let geo = window.geometry();
            let bbox = window.bbox();
            let size = bbox.size;
            let origin = checked_sub(location, geo.loc)?;
            let visual_origin = checked_add(origin, bbox.loc)?;
            let clip = state.layout.camera().rect_to_view(placement.clip);
            let fullscreen = state.layout.is_fullscreen(id);
            let fullscreen_clip = if fullscreen {
                clipped_page(clip, viewport)?
            } else {
                None
            };
            if fullscreen && fullscreen_clip.is_none() {
                continue;
            }
            if owners.len() >= MAX_SURFACES {
                return Err("scene owner budget exceeded".into());
            }
            owners.push((id, window.clone(), location, origin, fullscreen_clip));
            if size.w <= 0 || size.h <= 0 {
                continue;
            }
            let visual = Rect {
                x: f64::from(visual_origin.x),
                y: f64::from(visual_origin.y),
                width: f64::from(size.w),
                height: f64::from(size.h),
            };
            let Some(mut input) = clipped_root_input(id, visual, clip, viewport)? else {
                continue;
            };
            input.z_index = z as i32;
            input.focused = placement.focused;
            inputs.push(input);
        }
        let geometry = SceneSnapshot::new(viewport, inputs).map_err(|e| e.to_string())?;
        let viewport = rectangle(viewport)?;
        let mut input = Vec::new();
        let mut layers = Vec::new();
        let mut region_rects = 0;
        let visited = std::cell::Cell::new(0);
        let mut tree_count = 0;
        for (id, window, location, origin, fullscreen_clip) in owners.into_iter().rev() {
            let mut trees = Vec::new();
            let Some(top) = window.toplevel() else {
                continue;
            };
            // Popup eligibility follows mapped scene ownership, independently
            // of whether the root's own buffer intersects its container/output.
            for (popup, offset) in PopupManager::popups_for_surface(top.wl_surface()) {
                let offset = checked_sub(offset, popup.geometry().loc)?;
                let popup_origin = checked_add(location, offset)?;
                if tree_count >= MAX_SURFACES {
                    return Err("scene tree budget exceeded".into());
                }
                tree_count += 1;
                trees.push(Tree {
                    window: id,
                    popup: true,
                    fullscreen: fullscreen_clip.is_some(),
                    surface: popup.wl_surface().clone(),
                    origin: popup_origin,
                    clip: fullscreen_clip.unwrap_or(viewport),
                });
            }
            if let Some(entry) = geometry.entries().iter().find(|entry| entry.window() == id) {
                if tree_count >= MAX_SURFACES {
                    return Err("scene tree budget exceeded".into());
                }
                tree_count += 1;
                trees.push(Tree {
                    window: id,
                    popup: false,
                    fullscreen: fullscreen_clip.is_some(),
                    surface: top.wl_surface().clone(),
                    origin,
                    clip: rectangle(entry.effective_clip())?,
                });
            }
            for tree in &trees {
                let first = input.len();
                capture_input(tree, &mut input, &mut region_rects, &visited)?;
                layers.extend((first..input.len()).map(Layer::Surface));
            }
            // Fullscreen belongs to this frozen owner group: popups and the
            // whole client tree draw above it; every lower owner draws below.
            // Keep it even when the centered client lies outside the viewport.
            if let Some(clip) = fullscreen_clip {
                let buffer = SolidColorBuffer::new(clip.size, [0.0, 0.0, 0.0, 1.0]);
                let element = SolidColorRenderElement::from_buffer(
                    &buffer,
                    clip.loc.to_physical(1),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                );
                layers.push(Layer::Backdrop { clip, element });
            }
        }
        let popup_surfaces = input.iter().filter(|surface| surface.popup).count();
        Ok(Self {
            geometry,
            input,
            layers,
            popup_surfaces,
        })
    }

    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.input
            .iter()
            .filter(|node| {
                let bounds = Rectangle::new(node.origin, node.bounds.size);
                bounds.overlaps(node.clip)
            })
            .map(|node| node.window)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
    }

    pub fn render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Result<Vec<SceneRenderElement>, String> {
        let mut elements = Vec::new();
        for layer in &self.layers {
            let node = match layer {
                Layer::Surface(index) => &self.input[*index],
                Layer::Backdrop { element, .. } => {
                    elements.push(SceneRenderElement::Backdrop(element.clone()));
                    continue;
                }
            };
            // Propagate import failures rather than publishing input eligibility
            // for a surface silently omitted by the convenience tree helper.
            let element = with_states(&node.surface, |states| {
                WaylandSurfaceRenderElement::from_surface(
                    renderer,
                    &node.surface,
                    states,
                    node.origin.to_physical(1.0),
                    1.0,
                    Kind::Unspecified,
                )
            })
            .map_err(|error| format!("scene surface import failed: {error}"))?
            .ok_or("mapped scene surface produced no render element")?;
            if let Some(element) = CropRenderElement::from_element(
                element,
                1.0,
                node.clip
                    .to_i32_round::<i32>()
                    .to_physical_precise_round(1.0),
            ) {
                elements.push(if node.fullscreen {
                    SceneRenderElement::FullscreenSurface(FullscreenSurfaceRenderElement(element))
                } else {
                    SceneRenderElement::Surface(element)
                });
            }
        }
        Ok(elements)
    }

    pub fn hit(
        &self,
        pos: Point<f64, Logical>,
        state: &Fluxwm,
    ) -> Option<(WindowId, WlSurface, Point<f64, Logical>)> {
        if !pos.x.is_finite()
            || !pos.y.is_finite()
            || !self
                .geometry
                .viewport()
                .contains(ModelPoint { x: pos.x, y: pos.y })
        {
            return None;
        }
        for layer in &self.layers {
            let node = match layer {
                Layer::Surface(index) => &self.input[*index],
                Layer::Backdrop { clip, .. } => {
                    // The black pixels belong to this presented scene even if
                    // their old owner has since disappeared. Block click-through
                    // until a replacement frame removes the page.
                    if clip.to_f64().contains(pos) {
                        return None;
                    }
                    continue;
                }
            };
            if !owner_eligible(state, node.window)
                || !node.surface.alive()
                || !node.clip.contains(pos)
            {
                continue;
            }
            if node.popup {
                let Some(top) = state
                    .windows
                    .get(node.window)
                    .and_then(|item| item.window.toplevel())
                else {
                    continue;
                };
                if !PopupManager::popups_for_surface(top.wl_surface())
                    .any(|(popup, _)| popup.wl_surface() == &node.tree_root)
                {
                    continue;
                }
            }
            if !still_mapped_in_tree(&node.surface, &node.tree_root) {
                continue;
            }
            let local = pos - node.origin;
            if !node.bounds.contains(local)
                || node
                    .region
                    .as_ref()
                    .is_some_and(|r| !r.contains(local.to_i32_floor()))
            {
                continue;
            }
            return Some((node.window, node.surface.clone(), node.origin));
        }
        None
    }
}

fn owner_eligible(state: &Fluxwm, window: WindowId) -> bool {
    !state.layout.is_minimized(window)
        && state.windows.get(window).is_some_and(|item| {
            item.mapped
                && item
                    .window
                    .toplevel()
                    .is_some_and(|top| still_mapped_in_tree(top.wl_surface(), top.wl_surface()))
        })
}

// Intersect floating-point world projections before conversion, then use the
// same rounded rectangle for opaque pixels, popup clipping, and input blocking.
fn clipped_page(page: Rect, viewport: Rect) -> Result<Option<Rectangle<i32, Logical>>, String> {
    let Some(clip) = flux_scene::intersection(page, viewport).map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    Ok(round_rect_to_i32(clip)
        .map_err(|error| error.to_string())?
        .map(|rect| Rectangle::new((rect.x, rect.y).into(), (rect.width, rect.height).into())))
}

fn checked_sub(
    a: Point<i32, Logical>,
    b: Point<i32, Logical>,
) -> Result<Point<i32, Logical>, String> {
    Ok((
        a.x.checked_sub(b.x).ok_or("scene origin overflow")?,
        a.y.checked_sub(b.y).ok_or("scene origin overflow")?,
    )
        .into())
}
fn checked_add(
    a: Point<i32, Logical>,
    b: Point<i32, Logical>,
) -> Result<Point<i32, Logical>, String> {
    Ok((
        a.x.checked_add(b.x).ok_or("scene origin overflow")?,
        a.y.checked_add(b.y).ok_or("scene origin overflow")?,
    )
        .into())
}
fn rectangle(rect: Rect) -> Result<Rectangle<i32, Logical>, String> {
    let value = round_rect_to_i32(rect)
        .map_err(|e| e.to_string())?
        .ok_or("scene clip collapsed")?;
    Ok(Rectangle::new(
        (value.x, value.y).into(),
        (value.width, value.height).into(),
    ))
}

fn capture_input(
    tree: &Tree,
    nodes: &mut Vec<InputSurface>,
    region_rects: &mut usize,
    visited: &std::cell::Cell<usize>,
) -> Result<(), String> {
    let overflow = std::cell::Cell::new(false);
    with_surface_tree_downward(
        &tree.surface,
        (tree.origin.to_f64(), 0usize),
        |_, states, (origin, depth)| {
            if overflow.get() {
                return TraversalAction::SkipChildren;
            }
            if *depth >= MAX_TREE_DEPTH || visited.get() >= MAX_SURFACES {
                overflow.set(true);
                return TraversalAction::SkipChildren;
            }
            visited.set(visited.get() + 1);
            let view = states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .and_then(|d| d.lock().unwrap().view());
            match view {
                Some(view) => {
                    TraversalAction::DoChildren((*origin + view.offset.to_f64(), depth + 1))
                }
                None => TraversalAction::SkipChildren,
            }
        },
        |surface, states, (origin, _)| {
            if overflow.get() {
                return;
            }
            let Some(view) = states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .and_then(|d| d.lock().unwrap().view())
            else {
                return;
            };
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            let region = &attributes.current().input_region;
            let count = region.as_ref().map_or(0, |r| r.rects.len());
            if nodes.len() >= MAX_SURFACES || count > MAX_REGION_RECTS - *region_rects {
                overflow.set(true);
                return;
            }
            *region_rects += count;
            nodes.push(InputSurface {
                window: tree.window,
                tree_root: tree.surface.clone(),
                popup: tree.popup,
                fullscreen: tree.fullscreen,
                surface: surface.clone(),
                origin: *origin + view.offset.to_f64(),
                bounds: Rectangle::from_size(view.dst).to_f64(),
                clip: tree.clip.to_f64(),
                region: region.clone(),
            });
        },
        |_, _, _| !overflow.get(),
    );
    if overflow.get() {
        Err("scene surface/input-region budget exceeded".into())
    } else {
        Ok(())
    }
}

// Protocol lifecycle invalidates eligibility immediately, even while the last
// composed geometry remains on screen. Do not recompute geometry/input regions.
fn still_mapped_in_tree(surface: &WlSurface, anchor: &WlSurface) -> bool {
    let mut current = surface.clone();
    for _ in 0..MAX_TREE_DEPTH {
        if !current.alive() {
            return false;
        }
        let mapped = with_states(&current, |states| {
            states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .is_some_and(|data| data.lock().unwrap().view().is_some())
        });
        if !mapped {
            return false;
        }
        if &current == anchor {
            return true;
        }
        let Some(parent) = get_parent(&current) else {
            return false;
        };
        current = parent;
    }
    false
}

// Cull output-invisible roots before applying the snapshot's local coordinate
// limits. Legal far-away camera positions must not fail a local output build.
fn clipped_root_input(
    window: WindowId,
    visual: Rect,
    clip: Rect,
    viewport: Rect,
) -> Result<Option<SceneInput>, String> {
    let Some(clip) = flux_scene::intersection(clip, viewport).map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    if flux_scene::intersection(visual, clip)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Ok(None);
    }
    let Some(clip) = round_rect_to_i32(clip).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    Ok(Some(SceneInput::new(
        window,
        visual,
        Rect {
            x: f64::from(clip.x),
            y: f64::from(clip.y),
            width: f64::from(clip.width),
            height: f64::from(clip.height),
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn viewport() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        }
    }
    #[test]
    fn fullscreen_page_is_clipped_before_rounding_and_culled_independently() {
        let page = Rect {
            x: -2_000_000_000.0,
            width: 4_000_000_000.0,
            ..viewport()
        };
        assert_eq!(
            clipped_page(page, viewport()).unwrap(),
            Some(rectangle(viewport()).unwrap())
        );
        let offscreen = Rect {
            x: 2000.0,
            ..viewport()
        };
        assert!(clipped_page(offscreen, viewport()).unwrap().is_none());
        let edge = Rect {
            x: -1000.0,
            ..viewport()
        };
        let clip = clipped_page(edge, viewport()).unwrap().unwrap();
        assert_eq!(clip, Rectangle::new((0, 0).into(), (280, 800).into()));
        assert!(clip.to_f64().contains((279.999, 10.0)));
        assert!(!clip.to_f64().contains((280.0, 10.0)));
        // The centered client may be outside the visible page edge. The opaque
        // page itself must survive independently of root-buffer eligibility.
        assert!(
            clipped_root_input(WindowId::new(0, 0), offscreen, edge, viewport())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn offscreen_camera_limit_does_not_reject_the_local_scene() {
        let visual = Rect {
            x: 1_000_000_000.0,
            y: 0.0,
            width: 1900.0,
            height: 240.0,
        };
        let clip = Rect {
            width: 635.0,
            height: 800.0,
            ..visual
        };
        // The unprojected rectangle exceeds the strict local snapshot endpoint
        // bound, but is legal far-away layout content and must simply be culled.
        assert!(
            SceneSnapshot::new(
                viewport(),
                [SceneInput::new(WindowId::new(0, 0), visual, clip)]
            )
            .is_err()
        );
        assert!(
            clipped_root_input(WindowId::new(0, 0), visual, clip, viewport())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn container_projection_precedes_integer_conversion() {
        let visual = Rect {
            width: 1900.0,
            height: 240.0,
            ..viewport()
        };
        let clip = Rect {
            x: -2_000_000_000.0,
            width: 4_000_000_000.0,
            ..viewport()
        };
        assert!(round_rect_to_i32(clip).is_err());
        let input = clipped_root_input(WindowId::new(0, 0), visual, clip, viewport())
            .unwrap()
            .unwrap();
        let scene = SceneSnapshot::new(viewport(), [input]).unwrap();
        assert_eq!(scene.entries()[0].effective_clip(), viewport());
        assert_eq!(scene.entries()[0].visual_rect(), visual);
    }
    #[test]
    fn snapped_container_edge_has_the_same_half_open_input_boundary() {
        let visual = Rect {
            width: 1900.0,
            height: 240.0,
            ..viewport()
        };
        let clip = Rect {
            x: 0.4,
            width: 280.0,
            ..viewport()
        };
        let input = clipped_root_input(WindowId::new(0, 0), visual, clip, viewport())
            .unwrap()
            .unwrap();
        let scene = SceneSnapshot::new(viewport(), [input]).unwrap();
        assert!(
            scene
                .hit_test(ModelPoint {
                    x: 279.999,
                    y: 100.0
                })
                .is_some()
        );
        assert!(scene.hit_test(ModelPoint { x: 280.0, y: 100.0 }).is_none());
        assert_eq!(scene.entries()[0].visual_rect().width, 1900.0);
    }
}
