use super::*;
use std::{marker::PhantomData, rc::Rc};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN};

pub struct Device {
    pub d3d: ID3D11Device,
    pub commands: ID3D11DeviceContext,
    pub d2d: ID2D1DeviceContext,
    pub adapter_low: u32,
    pub adapter_high: i32,
    _thread: PhantomData<Rc<()>>,
}

impl Device {
    pub fn hardware(adapter_id: Option<(u32, i32)>) -> Result<Self, String> {
        #[cfg(test)]
        crate::test_resources::require_native_gpu();
        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.map_err(|error| error.to_string())?;
        let adapter: Option<IDXGIAdapter> = if let Some((low, high)) = adapter_id {
            (0..16)
                .find_map(|index| {
                    let adapter = unsafe { factory.EnumAdapters1(index) }.ok()?;
                    let desc = unsafe { adapter.GetDesc1() }.ok()?;
                    (desc.AdapterLuid.LowPart == low
                        && desc.AdapterLuid.HighPart == high
                        && desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0)
                        .then(|| adapter.cast::<IDXGIAdapter>().ok())
                        .flatten()
                })
                .ok_or("GPU composition adapter unavailable")?
                .into()
        } else {
            None
        };
        let mut d3d = None;
        let mut commands = None;
        unsafe {
            D3D11CreateDevice(
                adapter.as_ref(),
                if adapter.is_some() {
                    D3D_DRIVER_TYPE_UNKNOWN
                } else {
                    D3D_DRIVER_TYPE_HARDWARE
                },
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                Some(&mut commands),
            )
        }
        .map_err(|error| error.to_string())?;
        let d3d = d3d.ok_or("D3D11 returned no compositor device")?;
        let commands = commands.ok_or("D3D11 returned no compositor context")?;
        let dxgi: IDXGIDevice = d3d.cast().map_err(|error| error.to_string())?;
        let desc = unsafe { dxgi.GetAdapter().and_then(|adapter| adapter.GetDesc()) }
            .map_err(|error| error.to_string())?;
        let d2d = unsafe {
            D2D1CreateDevice(&dxgi, None)
                .and_then(|device| device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE))
        }
        .map_err(|error| error.to_string())?;
        Ok(Self {
            d3d,
            commands,
            d2d,
            adapter_low: desc.AdapterLuid.LowPart,
            adapter_high: desc.AdapterLuid.HighPart,
            _thread: PhantomData,
        })
    }

    pub fn texture(&self, width: u32, height: u32) -> Result<ID3D11Texture2D, String> {
        if width == 0
            || height == 0
            || width > 7680
            || height > 4320
            || u64::from(width) * u64::from(height) * 4 > MAX_BYTES as u64
        {
            return Err("GPU composition texture exceeds the 64 MiB limit".into());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            ..Default::default()
        };
        let mut texture = None;
        unsafe { self.d3d.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(|error| error.to_string())?;
        texture.ok_or_else(|| "D3D11 returned no compositor texture".into())
    }

    pub fn bitmap(&self, texture: &ID3D11Texture2D, target: bool) -> Result<ID2D1Bitmap1, String> {
        let surface: IDXGISurface = texture.cast().map_err(|error| error.to_string())?;
        unsafe {
            self.d2d.CreateBitmapFromDxgiSurface(
                &surface,
                Some(&bitmap_properties(if target {
                    D2D1_BITMAP_OPTIONS_TARGET
                } else {
                    D2D1_BITMAP_OPTIONS_NONE
                })),
            )
        }
        .map_err(|error| error.to_string())
    }

    pub fn upload(&self, width: u32, height: u32, bgra: &[u8]) -> Result<ID2D1Bitmap1, String> {
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("GPU bitmap size overflow")?;
        if width == 0 || height == 0 || bytes > MAX_BYTES || bytes != bgra.len() {
            return Err("invalid GPU bitmap pixels".into());
        }
        unsafe {
            self.d2d.CreateBitmap(
                D2D_SIZE_U { width, height },
                Some(bgra.as_ptr().cast()),
                width * 4,
                &bitmap_properties(D2D1_BITMAP_OPTIONS_NONE),
            )
        }
        .map_err(|error| error.to_string())
    }
}
