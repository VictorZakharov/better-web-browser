use super::*;

pub struct Fence(ID3D11Query);
impl Fence {
    pub fn submit(device: &Device) -> Result<Self, String> {
        let desc = D3D11_QUERY_DESC {
            Query: D3D11_QUERY_EVENT,
            MiscFlags: 0,
        };
        let mut query = None;
        unsafe { device.d3d.CreateQuery(&desc, Some(&mut query)) }
            .map_err(|error| error.to_string())?;
        let query = query.ok_or("D3D11 returned no completion query")?;
        unsafe {
            device.commands.End(&query);
            device.commands.Flush();
        }
        Ok(Self(query))
    }

    pub fn complete(&self, device: &Device) -> Result<bool, String> {
        // S_FALSE is a positive HRESULT but does not mean completion. Poll the
        // immediate context without flushing or waiting on the UI thread.
        // https://learn.microsoft.com/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-getdata
        let result = unsafe {
            (device.commands.vtable().GetData)(
                device.commands.as_raw(),
                self.0.as_raw(),
                std::ptr::null_mut(),
                0,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
            )
        };
        match result.0 {
            0 => Ok(true),
            1 => Ok(false),
            _ => Err(format!(
                "GPU composition completion query failed: {:#x}",
                result.0
            )),
        }
    }
}
