use super::*;
use crate::engine::webgl::{ApiVersion, backend_policy::Backend, core_entries::CoreEntries, gl};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::*;

#[test]
#[ignore = "single-owner hardware interop proof"]
fn shared_angle_surface_real_pixels_and_keyed_handoff() {
    crate::engine::webgl::session::run_native_test(|| {
        for backend in [Backend::Hardware(None)] {
            let native = NativeContext::for_backend(ApiVersion::Two, backend).unwrap();
            let surface = native.shared_surface(8, 4).unwrap();
            native.d3d_device().unwrap();
            assert!(crate::gpu_frame::try_acquire(&surface.mutex, 0).unwrap());
            native.make_surface_current(&surface).unwrap();
            unsafe {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::Disable(gl::SCISSOR_TEST);
                gl::ClearColor(0.0, 1.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Enable(gl::SCISSOR_TEST);
                gl::Scissor(0, 0, 4, 2);
                gl::ClearColor(1.0, 0.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Disable(gl::SCISSOR_TEST);
                gl::Flush();
            }
            native.make_current().unwrap();
            unsafe {
                surface.mutex.ReleaseSync(1).unwrap();
            }
            let mut device = None;
            let mut context = None;
            unsafe {
                D3D11CreateDevice(
                    None,
                    if backend == Backend::Software {
                        D3D_DRIVER_TYPE_WARP
                    } else {
                        D3D_DRIVER_TYPE_HARDWARE
                    },
                    Default::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
                .unwrap();
            }
            let device = device.unwrap();
            let context = context.unwrap();
            let mut texture: Option<ID3D11Texture2D> = None;
            unsafe {
                device
                    .OpenSharedResource(surface.handle(), &mut texture)
                    .unwrap();
            }
            let texture = texture.unwrap();
            let mutex: IDXGIKeyedMutex = texture.cast().unwrap();
            assert!(crate::gpu_frame::try_acquire(&mutex, 1).unwrap());
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                texture.GetDesc(&mut desc);
            }
            // Compose a real GPU source together with a foreground HUD rectangle.
            use windows::Win32::Graphics::Direct2D::{Common::*, *};
            use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface};
            let dxgi: IDXGIDevice = device.cast().unwrap();
            let d2d = unsafe { D2D1CreateDevice(&dxgi, None) }.unwrap();
            let d2d = unsafe { d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }.unwrap();
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: desc.Format,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let source: IDXGISurface = texture.cast().unwrap();
            let source =
                unsafe { d2d.CreateBitmapFromDxgiSurface(&source, Some(&properties)) }.unwrap();
            let mut target_desc = desc;
            target_desc.MiscFlags = 0;
            let mut target = None;
            unsafe {
                device
                    .CreateTexture2D(&target_desc, None, Some(&mut target))
                    .unwrap();
            }
            let target = target.unwrap();
            let target_surface: IDXGISurface = target.cast().unwrap();
            let target_properties = D2D1_BITMAP_PROPERTIES1 {
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                ..properties.clone()
            };
            let target_bitmap = unsafe {
                d2d.CreateBitmapFromDxgiSurface(&target_surface, Some(&target_properties))
            }
            .unwrap();
            unsafe {
                d2d.SetTarget(&target_bitmap);
                d2d.BeginDraw();
                d2d.DrawBitmap(
                    &source,
                    None,
                    1.0,
                    D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                    None,
                    None,
                );
                let blue = d2d
                    .CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        },
                        None,
                    )
                    .unwrap();
                d2d.FillRectangle(
                    &D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: 2.0,
                        bottom: 1.0,
                    },
                    &blue,
                );
                d2d.EndDraw(None, None).unwrap();
            }
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.MiscFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            unsafe {
                device
                    .CreateTexture2D(&desc, None, Some(&mut staging))
                    .unwrap();
            }
            let staging = staging.unwrap();
            unsafe {
                context.CopyResource(&staging, &target);
                context.Flush();
                mutex.ReleaseSync(0).unwrap();
            }
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
            }
            let mut rows = Vec::new();
            for y in 0..4 {
                let row = unsafe {
                    std::slice::from_raw_parts(
                        (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                        8 * 4,
                    )
                };
                rows.push(row.to_vec());
            }
            unsafe {
                context.Unmap(&staging, 0);
            }
            // Native D3D row zero is EGL's GL-bottom row. Frame export must flip once.
            assert_eq!(&rows[0][..4], &[255, 0, 0, 255]);
            assert_eq!(&rows[1][..4], &[0, 0, 255, 255]);
            assert_eq!(&rows[3][..4], &[0, 255, 0, 255]);
            assert!(crate::gpu_frame::try_acquire(&surface.mutex, 0).unwrap());
            unsafe {
                surface.mutex.ReleaseSync(0).unwrap();
            }
            // Core symbols are independently available; the presentation copy can use a GPU-only blit.
            CoreEntries::load().unwrap();
        }
    });
}
