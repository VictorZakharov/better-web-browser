//! Validation for nested retained-paint state groups.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Clip,
    Opacity,
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
}
