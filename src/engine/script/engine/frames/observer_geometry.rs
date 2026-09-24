//! Cross-document IntersectionObserver coordinate routing.
use super::*;
use crate::engine::RectF;
use crate::engine::script::binding_helpers::argument_id;
use crate::engine::script::style_host::intersection::{self, Clip, Geometry};

/// Resolve an observed target in the document that actually owns its layout.
/// The caller's numeric node handle can refer to a same-origin child wrapper,
/// but its own HostState never owns that child's layout snapshot.
pub(in crate::engine::script::engine) fn intersection_geometry(
    scope: &mut v8::PinScope,
    args: &[super::super::value::JsValue],
) -> Option<super::super::value::JsValue> {
    let context = scope.get_current_context();
    let tree = tree(context)?;
    let observer = super::super::node_wrappers::host(context)?;
    let (target, root, observer_document, target_document) = {
        let state = observer.borrow();
        let target = state.node(argument_id(args, 1))?;
        let root = state.node(argument_id(args, 2));
        let document = state.document_for(&target)?.id();
        (target, root, state.document.id(), document)
    };
    if target_document == observer_document {
        return None;
    }
    let children = tree.children.borrow();
    let owner = children
        .values()
        .find(|child| child.active.get() && child.host.borrow().document.id() == target_document)?;
    let root_document = root
        .as_ref()
        .and_then(|root| observer.borrow().document_for(root))
        .map(|doc| doc.id());
    let local_root = (root_document == Some(target_document))
        .then(|| root.clone())
        .flatten();
    let mut result =
        intersection::calculate(&mut owner.host.borrow_mut(), Some(target), local_root);
    if root_document == Some(target_document) {
        return Some(result.value());
    }
    if root.is_some() && root_document != Some(observer_document) {
        result.valid = false;
        result.root = RectF::default();
        return Some(result.value());
    }
    let mut document = target_document;
    let mut outer_frame = None;
    for _ in 0..8 {
        if document == observer_document {
            break;
        }
        let child = children
            .values()
            .find(|child| child.active.get() && child.host.borrow().document.id() == document)?;
        let embedding = child.host.borrow().embedding_rect?;
        let parent = child.parent_host.upgrade()?;
        let mut parent = parent.borrow_mut();
        parent.flush_layout_if_needed();
        let frame = intersection::viewport_rect(&mut parent, &child.element, embedding);
        translate_geometry(&mut result, frame.x, frame.y);
        result.clips.push(Clip {
            rect: frame,
            x: true,
            y: true,
            scroll: true,
        });
        document = child.parent_document;
        outer_frame = Some(child.element.clone());
    }
    if document != observer_document {
        return None;
    }
    if let Some(root) = root {
        if root.element().is_some() {
            let frame = outer_frame?;
            let outer =
                intersection::calculate(&mut observer.borrow_mut(), Some(frame), Some(root));
            result.valid &= outer.valid;
            result.root = outer.root;
            result.root_scroll = outer.root_scroll;
            result.clips.extend(outer.clips);
        } else {
            let state = observer.borrow();
            result.root = RectF {
                x: 0.0,
                y: 0.0,
                width: state.media_environment.viewport_width,
                height: state.media_environment.viewport_height,
            };
            result.root_scroll = true;
        }
    } else {
        let state = observer.borrow();
        result.root = RectF {
            x: 0.0,
            y: 0.0,
            width: state.media_environment.viewport_width,
            height: state.media_environment.viewport_height,
        };
        result.root_scroll = true;
    }
    Some(result.value())
}

fn translate_geometry(geometry: &mut Geometry, x: f32, y: f32) {
    geometry.mapped_target.x += x;
    geometry.mapped_target.y += y;
    for clip in &mut geometry.clips {
        clip.rect.x += x;
        clip.rect.y += y;
    }
}
