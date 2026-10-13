use super::*;

pub struct Target {
    pub texture: ID3D11Texture2D,
    pub bitmap: ID2D1Bitmap1,
    pub width: u32,
    pub height: u32,
}
impl Target {
    pub fn new(device: &Device, width: u32, height: u32) -> Result<Self, String> {
        let texture = device.texture(width, height)?;
        let bitmap = device.bitmap(&texture, true)?;
        Ok(Self {
            texture,
            bitmap,
            width,
            height,
        })
    }

    /// Explicit capture/verification only. Ordinary frame composition never maps CPU pixels.
    pub fn read_bgra(&self, device: &Device) -> Result<Vec<u8>, String> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            self.texture.GetDesc(&mut desc);
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.MiscFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut staging = None;
        unsafe { device.d3d.CreateTexture2D(&desc, None, Some(&mut staging)) }
            .map_err(|error| error.to_string())?;
        let staging = staging.ok_or("D3D11 returned no capture texture")?;
        unsafe {
            device.commands.CopyResource(&staging, &self.texture);
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            device
                .commands
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|error| error.to_string())?;
        let row = self.width as usize * 4;
        let result = if mapped.pData.is_null() || (mapped.RowPitch as usize) < row {
            Err("GPU capture returned an invalid row pitch".into())
        } else {
            let mut pixels = Vec::with_capacity(row * self.height as usize);
            for y in 0..self.height as usize {
                pixels.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(
                        (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                        row,
                    )
                });
            }
            Ok(pixels)
        };
        unsafe {
            device.commands.Unmap(&staging, 0);
        }
        result
    }
}
