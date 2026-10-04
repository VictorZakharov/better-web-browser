//! Real sample allocation and resolves, with bounded native query output.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
pub(super) const SAMPLES: u32 = 0x80a9;
const NUM_COUNTS: u32 = 0x9380;

impl WebGl {
    pub(super) fn multisample_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        match c.op.as_str() {
            "getInternalformatParameter" => {
                if c.u(0)? != gl::RENDERBUFFER || c.u(2)? != SAMPLES {
                    return Err(gl::INVALID_ENUM);
                }
                Ok(json!(self.sample_counts(c.u(1)?)?))
            }
            "renderbufferStorageMultisample" => self.multisample_storage(c),
            "blitFramebuffer" => self.blit_framebuffer(c),
            _ => Err(gl::INVALID_OPERATION),
        }
    }

    fn sample_counts(&mut self, format: u32) -> Result<Vec<i32>> {
        // The closed renderability table prevents unknown driver output shapes.
        self.core_renderbuffer_bytes(format)?;
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .internalformat_query;
        let mut count = 0;
        unsafe { function(gl::RENDERBUFFER, format, NUM_COUNTS, 1, &mut count) };
        self.driver_result()?;
        if !(0..=32).contains(&count) {
            return Err(gl::OUT_OF_MEMORY);
        }
        let mut values = vec![0; count as usize];
        if count != 0 {
            unsafe {
                function(
                    gl::RENDERBUFFER,
                    format,
                    SAMPLES,
                    count,
                    values.as_mut_ptr(),
                )
            };
            self.driver_result()?;
        }
        // Sample counts are small positive integers returned in descending order.
        // Keep malformed provider data out of subsequent allocation arithmetic.
        if values.iter().any(|value| !(1..=64).contains(value))
            || values.windows(2).any(|pair| pair[0] <= pair[1])
        {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(values)
    }

    fn multisample_storage(&mut self, c: &Command) -> Result<Value> {
        if c.u(0)? != gl::RENDERBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        self.objects.get(self.renderbuffer, Kind::Renderbuffer)?;
        let samples = c.n(1)?;
        let mut format = c.u(2)?;
        let (width, height) = (c.n(3)?, c.n(4)?);
        if samples < 0 || !(0..=4096).contains(&width) || !(0..=4096).contains(&height) {
            return Err(gl::INVALID_VALUE);
        }
        // WebGL2 retains the unsized alias only for zero-sample storage.
        if format == 0x84f9 {
            if samples != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            format = 0x88f0;
        }
        let bytes = self.core_renderbuffer_bytes(format)?;
        let integer = super::core_texture_formats::storage(format).is_ok_and(|storage| {
            [
                super::core_texture_formats::RED_INTEGER,
                super::core_texture_formats::RG_INTEGER,
                super::core_texture_formats::RGBA_INTEGER,
            ]
            .contains(&storage.base)
        });
        if samples > 0 && integer {
            return Err(gl::INVALID_OPERATION);
        }
        let native_samples = if samples == 0 {
            1
        } else {
            self.sample_counts(format)?
                .into_iter()
                .rev()
                .find(|value| *value >= samples)
                .ok_or(gl::INVALID_VALUE)? as usize
        };
        let allocation = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(bytes))
            .and_then(|n| n.checked_mul(native_samples))
            .ok_or(gl::OUT_OF_MEMORY)?;
        self.charge(0, allocation)?;
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .renderbuffer_multisample;
        unsafe { function(gl::RENDERBUFFER, samples, format, width, height) };
        self.driver_result()?;
        self.objects
            .get_mut(self.renderbuffer, Kind::Renderbuffer)?
            .renderbuffer_format = format;
        Ok(Value::Null)
    }

    fn blit_framebuffer(&mut self, c: &Command) -> Result<Value> {
        let mut rectangle = [0i32; 8];
        for (index, value) in rectangle.iter_mut().enumerate() {
            *value = c.n(index)?;
        }
        // WebGL2 §3.7.4 prohibits overflow of a rectangle's GLint extent even
        // when each endpoint individually fits. Compute the difference in i64.
        for (start, end) in [(0, 2), (1, 3), (4, 6), (5, 7)] {
            if (i64::from(rectangle[end]) - i64::from(rectangle[start])).abs() > i64::from(i32::MAX)
            {
                return Err(gl::INVALID_VALUE);
            }
        }
        let mask = c.u(8)?;
        let filter = c.u(9)?;
        if mask & !(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT) != 0 {
            return Err(gl::INVALID_VALUE);
        }
        if ![gl::NEAREST, gl::LINEAR].contains(&filter) {
            return Err(gl::INVALID_ENUM);
        }
        if filter == gl::LINEAR && mask & (gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT) != 0 {
            return Err(gl::INVALID_OPERATION);
        }
        self.validate_framebuffer()?;
        self.validate_read_framebuffer()?;
        let function = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?.blit;
        unsafe {
            function(
                rectangle[0],
                rectangle[1],
                rectangle[2],
                rectangle[3],
                rectangle[4],
                rectangle[5],
                rectangle[6],
                rectangle[7],
                mask,
                filter,
            )
        };
        self.driver_result()?;
        Ok(Value::Null)
    }
}
