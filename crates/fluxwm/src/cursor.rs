//! Native software cursor. Named requests currently use the built-in arrow.
use smithay::{
    backend::renderer::{
        Frame, Renderer, buffer_dimensions,
        element::{
            Kind,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            utils::CropRenderElement,
        },
        gles::{GlesRenderer, GlesTarget},
        sync::SyncPoint,
        utils::{RendererSurfaceStateUserData, draw_render_elements},
    },
    input::pointer::{CursorImageStatus, CursorImageSurfaceData},
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Size, Transform},
    wayland::compositor::{TraversalAction, with_states, with_surface_tree_downward},
};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

const MAX_CURSOR_SURFACES: usize = 64;
const MAX_CURSOR_DEPTH: usize = 32;

fn origin(
    pointer: Point<f64, Logical>,
    hotspot: Point<i32, Logical>,
) -> Option<Point<i32, Physical>> {
    if !pointer.x.is_finite() || !pointer.y.is_finite() {
        return None;
    }
    let x = pointer.x.round() - f64::from(hotspot.x);
    let y = pointer.y.round() - f64::from(hotspot.y);
    if x < f64::from(i32::MIN)
        || y < f64::from(i32::MIN)
        || x > f64::from(i32::MAX - 32)
        || y > f64::from(i32::MAX - 32)
        || x.abs() > 1_048_576.
        || y.abs() > 1_048_576.
    {
        return None;
    }
    Some((x as i32, y as i32).into())
}

pub fn paint(
    renderer: &mut GlesRenderer,
    target: &mut GlesTarget<'_>,
    size: Size<i32, Physical>,
    status: &CursorImageStatus,
    pointer: Point<f64, Logical>,
    last_warning: &mut Option<Instant>,
) -> Result<Option<SyncPoint>, String> {
    if matches!(status, CursorImageStatus::Hidden) {
        return Ok(None);
    }
    let bounds = Rectangle::from_size(size);
    let mut elements: Vec<CropRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>> =
        Vec::new();
    let hotspot = if let CursorImageStatus::Surface(surface) = status {
        if !surface.alive() {
            return Ok(None);
        }
        // Bound the tree before invoking Smithay's surface import traversal.
        let visited = Cell::new(0usize);
        let pixels = Cell::new(0u64);
        let over_budget = Cell::new(false);
        with_surface_tree_downward(
            surface,
            0usize,
            |_, states, depth| {
                visited.set(visited.get() + 1);
                if visited.get() > MAX_CURSOR_SURFACES || *depth >= MAX_CURSOR_DEPTH {
                    over_budget.set(true);
                    return TraversalAction::Break;
                }
                let Some(data) = states.data_map.get::<RendererSurfaceStateUserData>() else {
                    return TraversalAction::SkipChildren;
                };
                let data = data.lock().unwrap();
                let Some(view) = data.view() else {
                    return TraversalAction::SkipChildren;
                };
                let dimensions = data.buffer().and_then(|buffer| buffer_dimensions(buffer));
                let oversized = dimensions.is_none_or(|size| {
                    size.w <= 0 || size.h <= 0 || size.w > 1024 || size.h > 1024
                });
                if let Some(dimensions) = dimensions {
                    pixels.set(
                        pixels.get()
                            + u64::from(dimensions.w.max(0) as u32)
                                * u64::from(dimensions.h.max(0) as u32),
                    );
                }
                if oversized
                    || pixels.get() > 4_194_304
                    || view.dst.w > 1024
                    || view.dst.h > 1024
                    || view.offset.x.unsigned_abs() > 16_384
                    || view.offset.y.unsigned_abs() > 16_384
                {
                    over_budget.set(true);
                    return TraversalAction::Break;
                }
                TraversalAction::DoChildren(depth + 1)
            },
            |_, _, _| (),
            |_, _, _| !over_budget.get(),
        );
        if over_budget.get() {
            let now = Instant::now();
            if last_warning.is_none_or(|last| now.duration_since(last) >= Duration::from_secs(10)) {
                *last_warning = Some(now);
                tracing::warn!(
                    "native cursor hidden: surface tree exceeds cursor size or tree budget"
                );
            }
            return Ok(None);
        }
        with_states(surface, |states| {
            states
                .data_map
                .get::<CursorImageSurfaceData>()
                .map(|data| data.lock().unwrap().hotspot)
                .unwrap_or_default()
        })
    } else {
        (0, 0).into()
    };
    let Some(position) = origin(pointer, hotspot) else {
        return Ok(None);
    };
    if let CursorImageStatus::Surface(surface) = status {
        let tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(renderer, surface, position, 1., 1., Kind::Cursor);
        elements.extend(
            tree.into_iter()
                .filter_map(|element| CropRenderElement::from_element(element, 1., bounds)),
        );
    }
    let mut frame = renderer
        .render(target, size, Transform::Normal)
        .map_err(|e| e.to_string())?;
    if matches!(status, CursorImageStatus::Surface(_)) {
        draw_render_elements::<GlesRenderer, _, _>(&mut frame, 1., &elements, &[bounds])
            .map_err(|e| e.to_string())?;
    } else {
        // A crisp arrow with black outline and white fill, in output pixels.
        for row in 0..23 {
            let width = if row < 17 { 1 + row / 2 } else { 3 };
            let x = if row < 17 { 0 } else { 5 };
            let rect = Rectangle::new((position.x + x, position.y + row).into(), (width, 1).into());
            if let Some(clip) = rect.intersection(bounds) {
                frame
                    .draw_solid(
                        clip,
                        &[Rectangle::from_size(clip.size)],
                        [0., 0., 0., 1.].into(),
                    )
                    .map_err(|e| e.to_string())?;
                if row > 1 && row < 16 && width > 2 {
                    let inner = Rectangle::new(
                        (position.x + 1, position.y + row).into(),
                        (width - 2, 1).into(),
                    );
                    if let Some(clip) = inner.intersection(bounds) {
                        frame
                            .draw_solid(
                                clip,
                                &[Rectangle::from_size(clip.size)],
                                [1., 1., 1., 1.].into(),
                            )
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    frame.finish().map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotspot_origin_is_checked_before_integer_conversion() {
        assert_eq!(
            origin((12.6, 7.4).into(), (3, 2).into()),
            Some((10, 5).into())
        );
        assert_eq!(
            origin((0., 0.).into(), (20, 30).into()),
            Some((-20, -30).into())
        );
        for pointer in [(f64::NAN, 0.), (0., f64::INFINITY), (f64::MAX, 0.)] {
            assert_eq!(origin(pointer.into(), (0, 0).into()), None);
        }
        assert_eq!(origin((0., 0.).into(), (i32::MIN, 0).into()), None);
    }
}
