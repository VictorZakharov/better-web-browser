use super::*;

pub(super) fn source_reader(bytes: &[u8]) -> Result<IMFSourceReader, String> {
    let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }
        .map_err(|error| format!("create in-memory media stream: {error}"))?;
    let mut written = 0_u32;
    unsafe {
        stream
            .Write(
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                Some(&mut written),
            )
            .ok()
            .map_err(|error| format!("copy encoded media into memory stream: {error}"))?;
    }
    if written as usize != bytes.len() {
        return Err("in-memory media stream accepted a partial write".into());
    }
    unsafe {
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|error| format!("rewind in-memory media stream: {error}"))?;
    }
    let byte_stream = unsafe { MFCreateMFByteStreamOnStream(&stream) }
        .map_err(|error| format!("adapt memory stream for Media Foundation: {error}"))?;
    unsafe { MFCreateSourceReaderFromByteStream(&byte_stream, None) }
        .map_err(|error| format!("create Media Foundation Source Reader: {error}"))
}

pub(super) fn select_stream(
    reader: &IMFSourceReader,
    stream: u32,
    name: &str,
) -> Result<(), String> {
    unsafe { reader.SetStreamSelection(stream, true) }
        .map_err(|error| format!("select Media Foundation {name} stream: {error}"))
}

pub(super) fn seek_source_reader(
    reader: &IMFSourceReader,
    position_100ns: u64,
) -> Result<(), String> {
    let position = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                Anonymous: PROPVARIANT_0_0_0 {
                    hVal: position_100ns as i64,
                },
                ..Default::default()
            }),
        },
    };
    unsafe {
        reader
            .SetCurrentPosition(&GUID::zeroed(), &position)
            .map_err(|error| format!("seek Media Foundation Source Reader: {error}"))
    }
}

pub(super) fn verify_native_type(
    reader: &IMFSourceReader,
    stream: u32,
    expected_major: GUID,
    expected_subtype: GUID,
    name: &str,
) -> Result<(), String> {
    let native = unsafe { reader.GetNativeMediaType(stream, 0) }
        .map_err(|error| format!("read native {name} type: {error}"))?;
    let major = unsafe { native.GetGUID(&MF_MT_MAJOR_TYPE) }
        .map_err(|error| format!("read native {name} major type: {error}"))?;
    let subtype = unsafe { native.GetGUID(&MF_MT_SUBTYPE) }
        .map_err(|error| format!("read native {name} subtype: {error}"))?;
    if major != expected_major || subtype != expected_subtype {
        return Err(format!(
            "owned fixture did not expose expected {name} track"
        ));
    }
    Ok(())
}

pub(super) fn output_type(
    major: GUID,
    subtype: GUID,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, String> {
    let media_type = unsafe { MFCreateMediaType() }
        .map_err(|error| format!("create decoded output type: {error}"))?;
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &major)
            .and_then(|_| media_type.SetGUID(&MF_MT_SUBTYPE, &subtype))
            .map_err(|error| format!("configure decoded output type: {error}"))?;
    }
    Ok(media_type)
}
