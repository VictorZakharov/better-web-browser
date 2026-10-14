use super::*;
use better_web_browser::engine::affine::Affine2D;
use std::collections::HashMap;

enum Group {
    Clip(D2D_RECT_F),
    Layer(usize),
    Transform(Affine2D),
}
pub(super) struct Groups {
    context: ID2D1DeviceContext,
    stack: Vec<Group>,
    matrix: Affine2D,
    clip: D2D_RECT_F,
    skipped: usize,
    bytes: usize,
}

impl Groups {
    pub(super) fn new(device: &Device, width: u32, height: u32) -> Self {
        Self {
            context: device.d2d.clone(),
            stack: Vec::new(),
            matrix: Affine2D::IDENTITY,
            clip: D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: width as f32,
                bottom: height as f32,
            },
            skipped: 0,
            bytes: 0,
        }
    }
    pub(super) fn axis_aligned(&self) -> bool {
        self.matrix.0[1] == 0.0 && self.matrix.0[2] == 0.0
    }
    pub(super) fn balanced(&self) -> bool {
        self.stack.is_empty() && self.skipped == 0
    }
    pub(super) fn visible(&self, rect: D2D_RECT_F) -> bool {
        let world = self.world(rect);
        world.right > self.clip.left
            && world.bottom > self.clip.top
            && world.left < self.clip.right
            && world.top < self.clip.bottom
    }
    fn world(&self, rect: D2D_RECT_F) -> D2D_RECT_F {
        let bounds = self.matrix.bounds(better_web_browser::engine::RectF {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        });
        D2D_RECT_F {
            left: bounds.x,
            top: bounds.y,
            right: bounds.right(),
            bottom: bounds.bottom(),
        }
    }
    fn charge(&mut self, bounds: D2D_RECT_F) -> Result<usize, String> {
        let world = self.world(bounds);
        let width = (world.right.min(self.clip.right) - world.left.max(self.clip.left))
            .max(0.0)
            .ceil() as usize;
        let height = (world.bottom.min(self.clip.bottom) - world.top.max(self.clip.top))
            .max(0.0)
            .ceil() as usize;
        let bytes = width.saturating_mul(height).saturating_mul(4);
        if self.bytes.saturating_add(bytes) > 64 * 1024 * 1024 {
            return Err("GPU paint layers exceed the 64 MiB limit".into());
        }
        self.bytes += bytes;
        Ok(bytes)
    }
    pub(super) fn handle(
        &mut self,
        compositor: &mut Compositor,
        item: &DisplayItem,
        images: &HashMap<String, DecodedImage>,
        scale: f32,
        scroll: i32,
    ) -> Result<bool, String> {
        let begin = matches!(
            item,
            DisplayItem::BeginClip { .. }
                | DisplayItem::BeginTransform { .. }
                | DisplayItem::BeginOpacity { .. }
                | DisplayItem::BeginMask { .. }
        );
        let end = matches!(
            item,
            DisplayItem::EndClip { .. }
                | DisplayItem::EndTransform { .. }
                | DisplayItem::EndOpacity { .. }
                | DisplayItem::EndMask { .. }
        );
        if self.skipped > 0 {
            if begin {
                self.skipped += 1;
            }
            if end {
                self.skipped -= 1;
            }
            return Ok(true);
        }
        if self.stack.len() >= 64 && begin {
            return Err("GPU page group depth exceeds limit".into());
        }
        match item {
            DisplayItem::BeginTransform { matrix: local, .. } => {
                self.stack.push(Group::Transform(self.matrix));
                self.matrix =
                    self.matrix
                        .then_local(super::super::painting::affine::screen_matrix(
                            *local, scroll, 0, scale,
                        ));
                if self.matrix.inverse().is_none() {
                    return Err("GPU transform is not invertible".into());
                }
                transform(&compositor.device, self.matrix);
            }
            DisplayItem::EndTransform { .. } => {
                let Some(Group::Transform(previous)) = self.stack.pop() else {
                    return Err("unbalanced GPU transform".into());
                };
                self.matrix = previous;
                transform(&compositor.device, self.matrix);
            }
            DisplayItem::BeginClip { bounds } => {
                let rect = primitives::rect(*bounds, scroll, scale);
                if !self.visible(rect) {
                    self.skipped = 1;
                    return Ok(true);
                }
                if !self.axis_aligned() {
                    return Err("GPU rotated clips require bitmap fallback".into());
                }
                self.stack.push(Group::Clip(self.clip));
                let world = self.world(rect);
                self.clip = D2D_RECT_F {
                    left: self.clip.left.max(world.left),
                    top: self.clip.top.max(world.top),
                    right: self.clip.right.min(world.right),
                    bottom: self.clip.bottom.min(world.bottom),
                };
                unsafe {
                    self.context
                        .PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_ALIASED);
                }
            }
            DisplayItem::EndClip { .. } => {
                let Some(Group::Clip(previous)) = self.stack.pop() else {
                    return Err("unbalanced GPU clip".into());
                };
                self.clip = previous;
                unsafe {
                    self.context.PopAxisAlignedClip();
                }
            }
            DisplayItem::BeginOpacity { bounds, opacity } => {
                let rect = primitives::rect(*bounds, scroll, scale);
                if *opacity <= 0.0 || !self.visible(rect) {
                    self.skipped = 1;
                    return Ok(true);
                }
                let bytes = self.charge(rect)?;
                layer(&compositor.device, rect, *opacity, None);
                self.stack.push(Group::Layer(bytes));
            }
            DisplayItem::BeginMask { bounds, url } => {
                let rect = primitives::rect(*bounds, scroll, scale);
                if !self.visible(rect) {
                    self.skipped = 1;
                    return Ok(true);
                }
                let Some(mask) = images.get(url) else {
                    // Missing/over-budget masks hide their group in the native
                    // painter too. They do not invalidate unrelated Canvas pixels.
                    // https://www.w3.org/TR/css-masking-1/#the-mask-image
                    self.skipped = 1;
                    return Ok(true);
                };
                let bitmap = compositor.images.get(&compositor.device, url, mask, None)?;
                let mut properties = D2D1_BRUSH_PROPERTIES {
                    opacity: 1.0,
                    ..Default::default()
                };
                properties.transform.M11 = (rect.right - rect.left) / mask.width as f32;
                properties.transform.M22 = (rect.bottom - rect.top) / mask.height as f32;
                properties.transform.M31 = rect.left;
                properties.transform.M32 = rect.top;
                let image_properties = D2D1_IMAGE_BRUSH_PROPERTIES {
                    sourceRectangle: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: mask.width as f32,
                        bottom: mask.height as f32,
                    },
                    extendModeX: D2D1_EXTEND_MODE_CLAMP,
                    extendModeY: D2D1_EXTEND_MODE_CLAMP,
                    interpolationMode: D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                };
                let brush = unsafe {
                    self.context
                        .CreateImageBrush(&bitmap, &image_properties, Some(&properties))
                }
                .map_err(|error| error.to_string())?;
                let bytes = self.charge(rect)?;
                layer(
                    &compositor.device,
                    rect,
                    1.0,
                    Some(brush.cast().map_err(|error| error.to_string())?),
                );
                self.stack.push(Group::Layer(bytes));
            }
            DisplayItem::EndOpacity { .. } | DisplayItem::EndMask { .. } => {
                let Some(Group::Layer(bytes)) = self.stack.pop() else {
                    return Err("unbalanced GPU paint layer".into());
                };
                self.bytes -= bytes;
                unsafe {
                    self.context.PopLayer();
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

impl Drop for Groups {
    fn drop(&mut self) {
        while let Some(group) = self.stack.pop() {
            unsafe {
                match group {
                    Group::Clip(_) => self.context.PopAxisAlignedClip(),
                    Group::Layer(_) => self.context.PopLayer(),
                    Group::Transform(_) => {}
                }
            }
        }
    }
}

pub(super) fn identity(device: &Device) {
    let mut matrix = D2D1_LAYER_PARAMETERS1::default().maskTransform;
    matrix.M11 = 1.0;
    matrix.M22 = 1.0;
    unsafe {
        device.d2d.SetTransform(&matrix);
    }
}

pub(super) fn transform(device: &Device, value: Affine2D) {
    let [a, b, c, d, e, f] = value.0;
    let mut matrix = D2D1_LAYER_PARAMETERS1::default().maskTransform;
    matrix.M11 = a as f32;
    matrix.M12 = b as f32;
    matrix.M21 = c as f32;
    matrix.M22 = d as f32;
    matrix.M31 = e as f32;
    matrix.M32 = f as f32;
    unsafe {
        device.d2d.SetTransform(&matrix);
    }
}

pub(super) fn layer(device: &Device, bounds: D2D_RECT_F, opacity: f32, brush: Option<ID2D1Brush>) {
    let mut parameters = D2D1_LAYER_PARAMETERS1 {
        contentBounds: bounds,
        opacity: (opacity.clamp(0.0, 1.0) * 255.0).round() / 255.0,
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_ALIASED,
        ..Default::default()
    };
    parameters.maskTransform.M11 = 1.0;
    parameters.maskTransform.M22 = 1.0;
    parameters.opacityBrush = std::mem::ManuallyDrop::new(brush);
    unsafe {
        device.d2d.PushLayer(&parameters, None);
        std::mem::ManuallyDrop::drop(&mut parameters.opacityBrush);
    }
}
