use super::*;

pub(in crate::windows_app::rendering_resources::images) struct Target {
    pub(in crate::windows_app::rendering_resources::images) bitmap: Bitmap,
    pub(in crate::windows_app::rendering_resources::images) dc: Hdc,
    previous: Hgdiobj,
}
impl Target {
    pub(in crate::windows_app::rendering_resources::images) unsafe fn new(
        image: &DecodedImage,
    ) -> Self {
        unsafe {
            let bitmap = Bitmap::create(image, &image.bgra, null_mut()).unwrap();
            let dc = CreateCompatibleDC(null_mut());
            assert!(!dc.is_null());
            let previous = SelectObject(dc, bitmap.handle);
            assert!(!previous.is_null() && previous as isize != -1);
            Self {
                bitmap,
                dc,
                previous,
            }
        }
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        unsafe {
            GdiFlush();
            SelectObject(self.dc, self.previous);
            DeleteDC(self.dc);
        }
    }
}
