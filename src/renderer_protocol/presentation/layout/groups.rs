//! Validation for nested retained-paint state groups.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Clip,
    Opacity,
    Mask,
    Transform,
}

pub(super) fn validate_display_groups(items: &[DisplayItem]) -> Result<(), ProtocolError> {
    let mut stack = Vec::new();
    for item in items {
        let action = match item {
            DisplayItem::BeginClip { .. } => Some((true, GroupKind::Clip)),
            DisplayItem::EndClip { .. } => Some((false, GroupKind::Clip)),
            DisplayItem::BeginOpacity { opacity, .. } => {
                if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                    return Err(ProtocolError::InvalidPayload("opacity"));
                }
                Some((true, GroupKind::Opacity))
            }
            DisplayItem::EndOpacity { .. } => Some((false, GroupKind::Opacity)),
            DisplayItem::BeginMask { .. } => Some((true, GroupKind::Mask)),
            DisplayItem::EndMask { .. } => Some((false, GroupKind::Mask)),
            DisplayItem::BeginTransform { matrix, .. } => {
                if !super::affine::valid(*matrix) {
                    return Err(ProtocolError::InvalidPayload("affine matrix"));
                }
                Some((true, GroupKind::Transform))
            }
            DisplayItem::EndTransform { .. } => Some((false, GroupKind::Transform)),
            _ => None,
        };
        let Some((begin, kind)) = action else {
            continue;
        };
        if begin {
            if stack.len() >= 256 {
                return Err(ProtocolError::InvalidPayload("display group depth"));
            }
            stack.push(kind);
        } else if stack.pop() != Some(kind) {
            return Err(ProtocolError::InvalidPayload("display group balance"));
        }
    }
    if stack.is_empty() {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPayload("display group balance"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> RectF {
        RectF {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        }
    }

    #[test]
    fn accepts_properly_nested_display_groups() {
        let items = vec![
            DisplayItem::BeginClip { bounds: bounds() },
            DisplayItem::BeginOpacity {
                bounds: bounds(),
                opacity: 0.5,
            },
            DisplayItem::EndOpacity { bounds: bounds() },
            DisplayItem::EndClip { bounds: bounds() },
        ];
        assert!(validate_display_groups(&items).is_ok());
    }

    #[test]
    fn affine_groups_preserve_mixed_nesting_and_enforce_balance_and_depth() {
        let begin = DisplayItem::BeginTransform {
            bounds: bounds(),
            matrix: crate::engine::affine::Affine2D::IDENTITY,
        };
        let end = DisplayItem::EndTransform { bounds: bounds() };
        let clip = DisplayItem::BeginClip { bounds: bounds() };
        let unclip = DisplayItem::EndClip { bounds: bounds() };
        assert!(
            validate_display_groups(&[begin.clone(), clip.clone(), unclip.clone(), end.clone()])
                .is_ok()
        );
        assert!(validate_display_groups(&[begin.clone(), clip, end.clone(), unclip]).is_err());
        assert!(validate_display_groups(std::slice::from_ref(&begin)).is_err());
        assert!(validate_display_groups(std::slice::from_ref(&end)).is_err());
        let mut nested = vec![begin.clone(); 256];
        nested.extend(vec![end.clone(); 256]);
        assert!(validate_display_groups(&nested).is_ok());
        nested.insert(0, begin);
        nested.push(end);
        assert!(validate_display_groups(&nested).is_err());
    }

    #[test]
    fn rejects_crossed_display_groups() {
        let items = vec![
            DisplayItem::BeginClip { bounds: bounds() },
            DisplayItem::BeginOpacity {
                bounds: bounds(),
                opacity: 0.5,
            },
            DisplayItem::EndClip { bounds: bounds() },
            DisplayItem::EndOpacity { bounds: bounds() },
        ];
        assert!(validate_display_groups(&items).is_err());
    }
    #[test]
    fn masks_cannot_cross_clips_or_exceed_the_group_depth_budget() {
        let mask = DisplayItem::BeginMask {
            bounds: bounds(),
            url: "mask".into(),
        };
        assert!(
            validate_display_groups(&[
                mask.clone(),
                DisplayItem::BeginClip { bounds: bounds() },
                DisplayItem::EndMask { bounds: bounds() },
                DisplayItem::EndClip { bounds: bounds() }
            ])
            .is_err()
        );
        let mut items = vec![mask; 257];
        items.extend(vec![DisplayItem::EndMask { bounds: bounds() }; 257]);
        assert!(validate_display_groups(&items).is_err());
    }
}
