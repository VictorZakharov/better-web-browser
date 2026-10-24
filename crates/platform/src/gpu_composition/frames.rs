use super::*;
use crate::gpu_frame::{GpuFrame, PresentedGpuCanvas, try_acquire};
use std::collections::HashMap;
use windows::Win32::Foundation::HANDLE;

struct Shared {
    frame: GpuFrame,
    texture: ID3D11Texture2D,
    mutex: IDXGIKeyedMutex,
    used: u64,
}
struct Front {
    frame: GpuFrame,
    texture: ID3D11Texture2D,
    bitmap: ID2D1Bitmap1,
}
#[derive(Default)]
pub struct Frames {
    shared: HashMap<u64, Shared>,
    fronts: HashMap<String, Front>,
    epoch: u64,
}

impl Frames {
    pub fn ready(&self, values: &[PresentedGpuCanvas]) -> bool {
        values.iter().all(|value| {
            self.fronts
                .get(&value.url)
                .is_some_and(|front| front.frame == value.frame)
        })
    }
    pub fn bitmap(&self, url: &str) -> Option<&ID2D1Bitmap1> {
        self.fronts.get(url).map(|front| &front.bitmap)
    }

    pub fn update(
        &mut self,
        device: &Device,
        values: &[PresentedGpuCanvas],
    ) -> Result<bool, String> {
        if values.len() > crate::limits::MAX_PRESENTED_CANVASES {
            return Err("GPU Canvas count exceeds limit".into());
        }
        if values.iter().any(|value| !value.frame.validate()) {
            return Err("invalid GPU Canvas descriptor".into());
        }
        let total: usize = values
            .iter()
            .map(|value| value.frame.width as usize * value.frame.height as usize * 4)
            .sum();
        if total > MAX_BYTES {
            return Err("GPU Canvas fronts exceed the 64 MiB limit".into());
        }
        self.fronts
            .retain(|url, _| values.iter().any(|value| &value.url == url));
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("GPU import epoch exhausted")?;
        let mut changed = false;
        for value in values {
            let frame = value.frame;
            if !frame.validate()
                || (frame.adapter_low, frame.adapter_high)
                    != (device.adapter_low, device.adapter_high)
            {
                return Err("GPU Canvas descriptor or adapter mismatch".into());
            }
            if self
                .fronts
                .get(&value.url)
                .is_some_and(|front| front.frame == frame)
            {
                continue;
            }
            if !self.shared.contains_key(&frame.surface) {
                self.trim_shared(frame.width as usize * frame.height as usize * 4);
                let mut texture: Option<ID3D11Texture2D> = None;
                unsafe {
                    device
                        .d3d
                        .OpenSharedResource(HANDLE(frame.handle as usize as *mut _), &mut texture)
                }
                .map_err(|error| format!("open shared GPU Canvas: {error}"))?;
                let texture = texture.ok_or("D3D11 returned no shared Canvas texture")?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    texture.GetDesc(&mut desc);
                }
                if desc.Width != frame.width
                    || desc.Height != frame.height
                    || desc.MipLevels != 1
                    || desc.ArraySize != 1
                    || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                    || desc.SampleDesc.Count != 1
                    || desc.SampleDesc.Quality != 0
                    || desc.Usage != D3D11_USAGE_DEFAULT
                    || desc.CPUAccessFlags != 0
                    || desc.BindFlags
                        != (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32
                    || desc.MiscFlags != D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32
                {
                    return Err("shared GPU Canvas texture has an invalid descriptor".into());
                }
                let mutex = texture.cast().map_err(|error| error.to_string())?;
                self.shared.insert(
                    frame.surface,
                    Shared {
                        frame,
                        texture,
                        mutex,
                        used: self.epoch,
                    },
                );
            }
            let shared = self.shared.get_mut(&frame.surface).unwrap();
            if (shared.frame.handle, shared.frame.width, shared.frame.height)
                != (frame.handle, frame.width, frame.height)
            {
                return Err("GPU Canvas surface identity was redefined".into());
            }
            shared.used = self.epoch;
            if !try_acquire(&shared.mutex, frame.key)? {
                continue;
            }
            let copy = (|| {
                if self.fronts.get(&value.url).is_none_or(|front| {
                    (front.frame.width, front.frame.height) != (frame.width, frame.height)
                }) {
                    let texture = device.texture(frame.width, frame.height)?;
                    let bitmap = device.bitmap(&texture, false)?;
                    self.fronts.insert(
                        value.url.clone(),
                        Front {
                            frame,
                            texture,
                            bitmap,
                        },
                    );
                }
                let front = self.fronts.get_mut(&value.url).unwrap();
                unsafe {
                    device
                        .commands
                        .CopyResource(&front.texture, &shared.texture);
                    device.commands.Flush();
                }
                front.frame = frame;
                Ok::<_, String>(())
            })();
            let release = unsafe { shared.mutex.ReleaseSync(0) }.map_err(|error| error.to_string());
            copy?;
            release?;
            changed = true;
        }
        Ok(changed)
    }

    fn trim_shared(&mut self, incoming: usize) {
        let mut bytes: usize = self
            .shared
            .values()
            .map(|shared| shared.frame.width as usize * shared.frame.height as usize * 4)
            .sum();
        while bytes.saturating_add(incoming) > MAX_BYTES || self.shared.len() >= 192 {
            let Some(id) = self
                .shared
                .iter()
                .min_by_key(|(_, shared)| shared.used)
                .map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(shared) = self.shared.remove(&id) {
                bytes -= shared.frame.width as usize * shared.frame.height as usize * 4;
            }
        }
    }
}
