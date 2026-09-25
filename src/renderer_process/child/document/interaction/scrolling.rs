//! Wheel default actions target the nearest scrollable ancestor under the pointer.
use super::*;
use crate::engine::dom::Node;
use crate::renderer_protocol::WheelInput;

impl DocumentRuntime {
    pub(super) fn scroll_key(&mut self, key: &str) -> Result<Option<ScriptOutcome>, String> {
        let Some(id) = self.focused_node else {
            return Ok(None);
        };
        let Some(scroll) = self.layout.scroll_boxes.get(&id).copied() else {
            return Ok(None);
        };
        if !scroll.user_y {
            return Ok(None);
        }
        let value = match key {
            "ArrowUp" => scroll.offset_y - 40.0,
            "ArrowDown" => scroll.offset_y + 40.0,
            "PageUp" => scroll.offset_y - scroll.port.height,
            "PageDown" | " " => scroll.offset_y + scroll.port.height,
            "Home" => 0.0,
            "End" => scroll.content_height,
            _ => return Ok(None),
        };
        self.set_element_scroll(id, true, value).map(Some)
    }
    pub(super) fn scrollbar_pointer(
        &mut self,
        input: PointerInput,
    ) -> Result<Option<ScriptOutcome>, String> {
        if let Some((id, vertical, origin, offset)) = self.scroll_drag {
            if input.phase == PointerPhase::Up || input.buttons & 1 == 0 {
                self.scroll_drag = None;
            }
            if matches!(input.phase, PointerPhase::Move | PointerPhase::Up) {
                let Some(scroll) = self.layout.scroll_boxes.get(&id).copied() else {
                    return Ok(None);
                };
                let Some((track, thumb)) = scroll.scrollbar(vertical) else {
                    return Ok(None);
                };
                let (position, travel, maximum) = if vertical {
                    (
                        input.y,
                        track.height - thumb.height,
                        scroll.content_height - scroll.port.height,
                    )
                } else {
                    (
                        input.x,
                        track.width - thumb.width,
                        scroll.content_width - scroll.port.width,
                    )
                };
                let next = offset + (position - origin) * maximum / travel.max(1.0);
                return self.set_element_scroll(id, vertical, next).map(Some);
            }
        }
        if !matches!(input.phase, PointerPhase::Down | PointerPhase::Activate)
            || input.button != PointerButton::Primary
        {
            return Ok(None);
        }
        for id in self.layout.node_paint_order.iter().rev() {
            let Some(scroll) = self.layout.scroll_boxes.get(id) else {
                continue;
            };
            let Some(node) = self.page.dom.find_node(*id) else {
                continue;
            };
            if self.layout.hit_excluded.contains(&node.id())
                || !self.layout.point_in_scroll_clips(&node, input.x, input.y)
            {
                continue;
            }
            let Some(raw) = self.layout.node_bounds.get(id) else {
                continue;
            };
            let Some(visual) = self.layout.visual_rect(&node) else {
                continue;
            };
            let point = (input.x - visual.x + raw.x, input.y - visual.y + raw.y);
            for vertical in [true, false] {
                let Some((track, thumb)) = scroll.scrollbar(vertical) else {
                    continue;
                };
                if !contains(track, point.0, point.1) {
                    continue;
                }
                let id = *id;
                let offset = if vertical {
                    scroll.offset_y
                } else {
                    scroll.offset_x
                };
                self.focused_node = Some(id);
                self.pointer_down = [None; 3];
                if contains(thumb, point.0, point.1) {
                    if input.phase == PointerPhase::Down {
                        self.scroll_drag = Some((
                            id,
                            vertical,
                            if vertical { input.y } else { input.x },
                            offset,
                        ));
                    }
                    return Ok(Some(ScriptOutcome::default()));
                }
                let step = if vertical {
                    scroll.port.height
                } else {
                    scroll.port.width
                };
                let before = if vertical {
                    point.1 < thumb.y
                } else {
                    point.0 < thumb.x
                };
                return self
                    .set_element_scroll(id, vertical, offset + if before { -step } else { step })
                    .map(Some);
            }
        }
        Ok(None)
    }

    fn set_element_scroll(
        &mut self,
        id: NodeId,
        vertical: bool,
        value: f32,
    ) -> Result<ScriptOutcome, String> {
        let Some(node) = self.page.dom.find_node(id) else {
            return Ok(ScriptOutcome::default());
        };
        let Some(scroll) = self.layout.scroll_boxes.get(&id) else {
            return Ok(ScriptOutcome::default());
        };
        let offset = scroll.clamp(
            if vertical { scroll.offset_x } else { value },
            if vertical { value } else { scroll.offset_y },
        );
        if node.scroll_offset.replace(offset) == offset {
            return Ok(ScriptOutcome::default());
        }
        let mut result = self
            .dispatch_user_input(UserInputEvent::ElementScroll { target: node })?
            .outcome;
        result.render_requested = true;
        self.geometry_observers_pending = true;
        Ok(result)
    }

    pub(super) fn wheel_input(&mut self, input: WheelInput) -> Result<ScriptOutcome, String> {
        let target = self.hit_target(input.x, input.y).map(|target| target.node);
        let result = self.dispatch_user_input(UserInputEvent::Wheel {
            target: target.clone(),
            x: input.x,
            y: input.y,
            delta_x: input.delta_x,
            delta_y: input.delta_y,
            modifiers: input.modifiers.into(),
        })?;
        let mut outcome = result.outcome;
        if !result.default_allowed {
            return Ok(outcome);
        }
        if outcome.render_requested {
            self.page.refresh_resources_after_invalidation_for_viewport(
                self.viewport.style_width,
                self.viewport.height,
                &outcome.invalidation,
            );
            self.rebuild_layout();
        }
        let mut current = target;
        while let Some(node) = current {
            if let Some(scroll) = self.layout.scroll_boxes.get(&node.id()).copied()
                && scroll.can_scroll(input.delta_x, input.delta_y)
            {
                let offset = scroll.clamp(
                    scroll.offset_x + if scroll.user_x { input.delta_x } else { 0.0 },
                    scroll.offset_y + if scroll.user_y { input.delta_y } else { 0.0 },
                );
                node.scroll_offset.set(offset);
                let event =
                    self.dispatch_user_input(UserInputEvent::ElementScroll { target: node })?;
                merge_outcome(&mut outcome, event.outcome, self.page.dom.document.id());
                outcome.render_requested = true;
                self.geometry_observers_pending = true;
                return Ok(outcome);
            }
            current = Node::composed_parent(&node);
        }
        // The browser may still be animating earlier wheel inputs. An absolute position based
        // on input.viewport_y loses distance when multiple events were queued at that position.
        outcome.viewport_wheel_delta_y = input.delta_y;
        Ok(outcome)
    }
}
