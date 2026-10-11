//! Preserve GDI's device sampling phase before optional image-cache admission.
use super::{IdentityScope, Rect};
use crate::windows_app::platform::Hdc;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    DPtoLP, GetLayout, GetMapMode, GetWorldTransform, LPtoDP, MM_TEXT, XFORM,
};

pub(in crate::windows_app) struct ScaledDestination {
    rect: Rect,
    _identity: IdentityScope,
}

impl ScaledDestination {
    pub(in crate::windows_app) unsafe fn new(dc: Hdc, rect: Rect) -> Option<Self> {
        let mut matrix: XFORM = std::mem::zeroed();
        if GetWorldTransform(dc, &mut matrix) == 0
            || GetMapMode(dc) != MM_TEXT
            || GetLayout(dc) != 0
            || matrix.eM12 != 0.0
            || matrix.eM21 != 0.0
            || matrix.eM11 <= 0.0
            || matrix.eM22 <= 0.0
            || ![matrix.eM11, matrix.eM22, matrix.eDx, matrix.eDy]
                .iter()
                .all(|v| v.is_finite())
            || (matrix.eM11 == 1.0 && matrix.eM22 == 1.0)
        {
            return None;
        }
        // AlphaBlend transforms the destination through this DC. Resolve that
        // rectangle using GDI itself, not CSS bounds or independently rounded
        // floating-point products. Unsupported rotation/shear/reflection keeps
        // direct native painting. Mapping modes/RTL also retain the old path.
        // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-alphablend
        let mut points = [
            POINT {
                x: rect.left,
                y: rect.top,
            },
            POINT {
                x: rect.right,
                y: rect.bottom,
            },
        ];
        if LPtoDP(dc, points.as_mut_ptr(), 2) == 0 {
            return None;
        }
        let identity = IdentityScope::new(dc)?;
        // The borrowed DC can have a dirty-region/layer viewport offset. With
        // its world matrix neutralized, return to that same coordinate system;
        // its existing device clip and all other state remain in force.
        if DPtoLP(dc, points.as_mut_ptr(), 2) == 0 {
            return None;
        }
        let rect = Rect {
            left: points[0].x,
            top: points[0].y,
            right: points[1].x,
            bottom: points[1].y,
        };
        (rect.width() > 0 && rect.height() > 0).then_some(Self {
            rect,
            _identity: identity,
        })
    }

    pub(in crate::windows_app) fn rect(&self) -> Rect {
        self.rect
    }
}
