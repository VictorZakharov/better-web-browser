//! Route wheel coordinates to the renderer; only it knows the nested scrolling tree.
use super::*;
use better_web_browser::renderer_protocol::WheelInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) enum ContentWheelRoute {
    Outside,
    Submitted,
    Rejected,
}

impl ContentWheelRoute {
    fn from_submission(accepted: bool) -> Self {
        if accepted {
            Self::Submitted
        } else {
            Self::Rejected
        }
    }

    pub(in crate::windows_app) fn allows_viewport_fallback(self) -> bool {
        self == Self::Outside
    }
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn route_content_wheel(
        &mut self,
        delta: i32,
        wparam: Wparam,
        lparam: Lparam,
    ) -> ContentWheelRoute {
        if self.surface != Surface::Page {
            return ContentWheelRoute::Outside;
        }
        let mut point = Point {
            x: (lparam as u16) as i16 as i32,
            y: ((lparam >> 16) as u16) as i16 as i32,
        };
        ScreenToClient(self.window, &mut point);
        let locked_target = self.locked_pointer_owner().map(|owner| owner.target());
        if let Some(owner) = self.locked_pointer_owner() {
            point = owner.anchor_client();
        }
        let toolbar = self.toolbar_height();
        if point.x < 0 || point.y < toolbar || point.y > toolbar + self.viewport_height() {
            return ContentWheelRoute::Outside;
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return ContentWheelRoute::Outside;
        };
        let scale = self.page_scale().max(f32::EPSILON);
        let distance = -(delta as f32) * 126.0 / 120.0;
        let modifiers = pointer_modifiers(wparam);
        // Once content owns this wheel, rejection is not permission to bypass
        // its listeners/nested tree with a raw viewport default in WM_MOUSEWHEEL.
        let accepted = self.submit_renderer_input(DocumentInput::Wheel(WheelInput {
            document,
            sequence,
            x: point.x as f32 / scale,
            y: (point.y - toolbar + self.scroll_y) as f32 / scale,
            viewport_y: self.scroll_y as f32 / scale,
            delta_x: if modifiers.shift { distance } else { 0.0 },
            delta_y: if modifiers.shift { 0.0 } else { distance },
            modifiers,
            target: locked_target,
        }));
        ContentWheelRoute::from_submission(accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_backpressure_cannot_bypass_dom_default_action_routing() {
        assert!(ContentWheelRoute::Outside.allows_viewport_fallback());
        assert!(!ContentWheelRoute::from_submission(true).allows_viewport_fallback());
        assert!(!ContentWheelRoute::from_submission(false).allows_viewport_fallback());
        assert_eq!(
            ContentWheelRoute::from_submission(false),
            ContentWheelRoute::Rejected
        );
    }

    #[test]
    fn a_full_input_queue_rejects_the_reverse_without_restarting_old_defaults() {
        use crate::windows_app::renderer_input_queue::PendingRendererInputs;
        use crate::windows_app::scrolling::WheelGesture;
        use better_web_browser::limits::MAX_PENDING_RENDERER_INPUTS;
        use better_web_browser::renderer_protocol::{
            DocumentId, RuntimeReport, WheelAcknowledgement, WheelDecision,
        };

        for direction in [-1.0, 1.0] {
            let document = DocumentId::new(1).unwrap();
            let mut pending = PendingRendererInputs::default();
            let mut gesture = WheelGesture::default();
            let input = |sequence, delta_y| {
                DocumentInput::Wheel(WheelInput {
                    document,
                    sequence,
                    x: 10.0,
                    y: 10.0,
                    viewport_y: 0.0,
                    delta_x: 0.0,
                    delta_y,
                    modifiers: InputModifiers::default(),
                    target: None,
                })
            };
            for sequence in 1..=MAX_PENDING_RENDERER_INPUTS as u64 {
                gesture.observe(sequence, direction * 126.0);
                assert_eq!(
                    pending.enqueue(input(sequence, direction * 126.0)),
                    QueueResult::Queued
                );
            }
            let reverse = MAX_PENDING_RENDERER_INPUTS as u64 + 1;
            assert!(gesture.observe(reverse, -direction * 126.0));
            assert_eq!(
                pending.enqueue(input(reverse, -direction * 126.0)),
                QueueResult::Full
            );
            assert!(!ContentWheelRoute::from_submission(false).allows_viewport_fallback());
            for sequence in 1..reverse {
                assert_eq!(pending.pop_front().unwrap().sequence(), sequence);
                let old = RuntimeReport {
                    viewport_wheel_delta_y: direction * 126.0,
                    wheel_acknowledgements: vec![WheelAcknowledgement {
                        sequence,
                        decision: WheelDecision::Viewport,
                        viewport_delta_y: direction * 126.0,
                        dispatch_micros: 0,
                    }],
                    ..RuntimeReport::default()
                };
                assert_eq!(gesture.viewport_delta(&old), 0.0);
            }
            assert!(pending.is_empty());
        }
    }
}
