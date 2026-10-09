//! A pixel-only request is narrower than a normal rendering checkpoint.
//! Coalescing never downgrades geometry/state work to a surface repaint.

use super::EventLoopScheduler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderScope {
    Layout,
    SurfacePixels,
}

#[derive(Debug, Default)]
pub(super) struct RenderRequests(Option<RenderScope>);

impl RenderRequests {
    fn request(&mut self, scope: RenderScope) -> bool {
        let first = self.0.is_none();
        self.0 = Some(match self.0 {
            Some(RenderScope::Layout) => RenderScope::Layout,
            _ => scope,
        });
        first
    }
}

impl<T> EventLoopScheduler<T> {
    /// Requests a normal checkpoint, returning true only for the first request.
    pub fn request_render(&mut self) -> bool {
        self.rendering.request(RenderScope::Layout)
    }

    /// Native Canvas dirtiness does not itself invalidate style or box geometry.
    /// The renderer must still validate bitmap dimensions and retained paint.
    pub(crate) fn request_surface_repaint(&mut self) -> bool {
        self.rendering.request(RenderScope::SurfacePixels)
    }

    pub fn render_requested(&self) -> bool {
        self.rendering.0.is_some()
    }

    pub fn take_render_request(&mut self) -> bool {
        self.take_render_scope().is_some()
    }

    pub(crate) fn take_render_scope(&mut self) -> Option<RenderScope> {
        self.rendering.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_render_dominates_pixels_in_either_request_order() {
        for pixels_first in [false, true] {
            let mut scheduler = EventLoopScheduler::<()>::new();
            if pixels_first {
                assert!(scheduler.request_surface_repaint());
                assert!(!scheduler.request_render());
            } else {
                assert!(scheduler.request_render());
                assert!(!scheduler.request_surface_repaint());
            }
            assert_eq!(scheduler.take_render_scope(), Some(RenderScope::Layout));
            assert!(!scheduler.render_requested());
        }
    }

    #[test]
    fn pixels_coalesce_and_clear_and_legacy_take_consume_the_scope() {
        let mut scheduler = EventLoopScheduler::<()>::new();
        assert!(scheduler.request_surface_repaint());
        assert!(!scheduler.request_surface_repaint());
        assert_eq!(
            scheduler.take_render_scope(),
            Some(RenderScope::SurfacePixels)
        );
        assert!(!scheduler.take_render_request());
        scheduler.request_surface_repaint();
        assert!(scheduler.take_render_request());
        assert_eq!(scheduler.take_render_scope(), None);
        scheduler.request_surface_repaint();
        scheduler.clear();
        assert_eq!(scheduler.take_render_scope(), None);
    }
}
