use super::*;
use windows::Win32::{
    Foundation::HWND,
    Graphics::{DirectComposition::*, Dxgi::Common::*, Dxgi::*},
};

pub(super) struct Presentation {
    swap: IDXGISwapChain1,
    device: IDCompositionDevice,
    target: IDCompositionTarget,
    visual: IDCompositionVisual,
    bound: bool,
}

impl Presentation {
    pub(super) fn new(
        device: &Device,
        window: Hwnd,
        width: u32,
        height: u32,
        top: i32,
    ) -> Result<Self, String> {
        let dxgi: IDXGIDevice = device.d3d.cast().map_err(|error| error.to_string())?;
        let adapter = unsafe { dxgi.GetAdapter() }.map_err(|error| error.to_string())?;
        let factory: IDXGIFactory2 =
            unsafe { adapter.GetParent() }.map_err(|error| error.to_string())?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        let swap = unsafe { factory.CreateSwapChainForComposition(&device.d3d, &desc, None) }
            .map_err(|error| error.to_string())?;
        let composition: IDCompositionDevice =
            unsafe { DCompositionCreateDevice(&dxgi) }.map_err(|error| error.to_string())?;
        // The content visual stays below native child controls and cannot cover chrome.
        let target = unsafe { composition.CreateTargetForHwnd(HWND(window), false) }
            .map_err(|error| error.to_string())?;
        let visual = unsafe { composition.CreateVisual() }.map_err(|error| error.to_string())?;
        unsafe {
            visual
                .SetContent(&swap)
                .map_err(|error| error.to_string())?;
            visual
                .SetOffsetY2(top as f32)
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            swap,
            device: composition,
            target,
            visual,
            bound: false,
        })
    }

    pub(super) fn position(&self, top: i32) -> Result<(), String> {
        unsafe {
            self.visual
                .SetOffsetY2(top as f32)
                .and_then(|_| self.device.Commit())
        }
        .map_err(|error| error.to_string())
    }

    pub(super) fn resize(&self, width: u32, height: u32) -> Result<(), String> {
        unsafe {
            self.swap.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(0),
            )
        }
        .map_err(|error| error.to_string())
    }

    pub(super) fn present(&mut self, device: &Device, target: &Target) -> Result<bool, String> {
        let back: windows::Win32::Graphics::Direct3D11::ID3D11Texture2D =
            unsafe { self.swap.GetBuffer(0) }.map_err(|error| error.to_string())?;
        unsafe {
            device.commands.CopyResource(&back, &target.texture);
        }
        // Never synchronize the UI thread to a vertical refresh or queue an unbounded backlog.
        let result = unsafe { self.swap.Present(0, DXGI_PRESENT_DO_NOT_WAIT) };
        if result.0 == DXGI_ERROR_WAS_STILL_DRAWING.0 {
            return Ok(false);
        }
        result.ok().map_err(|error| error.to_string())?;
        if !self.bound {
            unsafe {
                self.target
                    .SetRoot(&self.visual)
                    .and_then(|_| self.device.Commit())
            }
            .map_err(|error| error.to_string())?;
            self.bound = true;
        }
        Ok(true)
    }
}

impl Drop for Presentation {
    fn drop(&mut self) {
        unsafe {
            let _ = self.target.SetRoot(None);
            let _ = self.device.Commit();
        }
    }
}
