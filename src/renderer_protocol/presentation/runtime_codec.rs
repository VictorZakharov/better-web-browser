//! Bounded serialization of document runtime updates, independent of presentation pixels.
use super::super::wire::{WireReader, WireWriter};
use super::{HistoryUpdate, MediaRuntimeReport, ProtocolError, RuntimeReport};
use crate::limits::{MAX_RUNTIME_REPORT_ENTRIES, MAX_RUNTIME_REPORT_TEXT_BYTES, MAX_URL_BYTES};

pub(in crate::renderer_protocol) fn encode_runtime(
    writer: &mut WireWriter,
    report: &RuntimeReport,
) -> Result<(), ProtocolError> {
    writer.u64(report.scripts_executed);
    writer.u64(report.dom_mutations);
    encode_strings(writer, &report.errors, "runtime errors count")?;
    encode_strings(writer, &report.console, "runtime console count")?;
    encode_strings(writer, &report.diagnostics, "runtime diagnostics count")?;
    writer.bool(report.navigation_url.is_some());
    if let Some(url) = &report.navigation_url {
        writer.string(url)?;
        let options = &report.navigation_options;
        options.validate().map_err(ProtocolError::InvalidPayload)?;
        writer.string(&options.target)?;
        writer.bool(options.noreferrer);
        writer.bool(options.replace_history);
        writer.bool(options.user_initiated);
        writer.bool(options.form_submission);
        writer.bool(options.post.is_some());
        if let Some(post) = &options.post {
            writer.string(&post.content_type)?;
            writer.bytes(&post.body)?;
        }
    }
    writer.bool(report.viewport_scroll_y.is_some());
    if let Some(y) = report.viewport_scroll_y {
        if !y.is_finite() || y < 0.0 {
            return Err(ProtocolError::InvalidPayload("viewport scroll offset"));
        }
        writer.f32(y);
    }
    if !report.viewport_wheel_delta_y.is_finite() {
        return Err(ProtocolError::InvalidPayload("viewport wheel delta"));
    }
    writer.f32(report.viewport_wheel_delta_y);
    if report.history_updates.len() > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("history update count"));
    }
    writer.u32(report.history_updates.len() as u32);
    for update in &report.history_updates {
        writer.string(&update.url)?;
        writer.bool(update.replace);
    }
    encode_strings(
        writer,
        &report.cookie_updates,
        "runtime cookie updates count",
    )?;
    writer.bool(report.runtime_active);
    writer.bool(report.runtime_stopped);
    writer.bool(report.render_requested);
    writer.bool(report.media.is_some());
    if let Some(media) = &report.media {
        encode_media_runtime(writer, media)?;
    }
    Ok(())
}

pub(in crate::renderer_protocol) fn decode_runtime(
    reader: &mut WireReader<'_>,
) -> Result<RuntimeReport, ProtocolError> {
    let scripts_executed = reader.u64()?;
    let dom_mutations = reader.u64()?;
    let errors = decode_strings(reader)?;
    let console = decode_strings(reader)?;
    let diagnostics = decode_strings(reader)?;
    let navigation_url = reader
        .bool()?
        .then(|| reader.string(MAX_URL_BYTES))
        .transpose()?;
    let navigation_options = if navigation_url.is_some() {
        use crate::navigation::request::{FormPost, MAX_FORM_BODY_BYTES, NavigationOptions};
        let options = NavigationOptions {
            target: reader.string(1024)?,
            noreferrer: reader.bool()?,
            replace_history: reader.bool()?,
            user_initiated: reader.bool()?,
            form_submission: reader.bool()?,
            post: if reader.bool()? {
                Some(FormPost {
                    content_type: reader.string(256)?,
                    body: reader.bytes(MAX_FORM_BODY_BYTES)?,
                })
            } else {
                None
            },
        };
        options.validate().map_err(ProtocolError::InvalidPayload)?;
        options
    } else {
        Default::default()
    };
    let viewport_scroll_y = reader.bool()?.then(|| reader.f32()).transpose()?;
    if viewport_scroll_y.is_some_and(|y| !y.is_finite() || y < 0.0) {
        return Err(ProtocolError::InvalidPayload("viewport scroll offset"));
    }
    let viewport_wheel_delta_y = reader.f32()?;
    if !viewport_wheel_delta_y.is_finite() {
        return Err(ProtocolError::InvalidPayload("viewport wheel delta"));
    }
    let history_update_count = reader.u32()? as usize;
    if history_update_count > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("history update count"));
    }
    let mut history_updates = Vec::with_capacity(history_update_count);
    for _ in 0..history_update_count {
        history_updates.push(HistoryUpdate {
            url: reader.string(MAX_URL_BYTES)?,
            replace: reader.bool()?,
        });
    }
    let cookie_updates = decode_strings(reader)?;
    let runtime_active = reader.bool()?;
    let runtime_stopped = reader.bool()?;
    let render_requested = reader.bool()?;
    let media = reader
        .bool()?
        .then(|| decode_media_runtime(reader))
        .transpose()?;
    Ok(RuntimeReport {
        scripts_executed,
        dom_mutations,
        errors,
        console,
        diagnostics,
        navigation_url,
        navigation_options,
        viewport_scroll_y,
        viewport_wheel_delta_y,
        history_updates,
        cookie_updates,
        runtime_active,
        runtime_stopped,
        render_requested,
        media,
    })
}

fn encode_media_runtime(
    writer: &mut WireWriter,
    report: &MediaRuntimeReport,
) -> Result<(), ProtocolError> {
    writer.bool(report.active);
    writer.bool(report.playing);
    writer.bool(report.ended);
    writer.u64(report.current_time_100ns);
    writer.u64(report.duration_100ns);
    for value in [
        &report.backend,
        &report.mime_type,
        &report.video_codec,
        &report.audio_codec,
    ] {
        if value.len() > MAX_RUNTIME_REPORT_TEXT_BYTES {
            return Err(ProtocolError::InvalidPayload("media runtime text"));
        }
        writer.string(value)?;
    }
    writer.u64(report.encoded_queue_bytes);
    writer.u64(report.encoded_queue_limit_bytes);
    writer.u16(report.decoded_frame_queue_depth);
    writer.u16(report.decoded_frame_queue_limit);
    writer.u64(report.frames_submitted);
    writer.u64(report.dropped_frames);
    writer.u32(report.width);
    writer.u32(report.height);
    writer.bool(report.failure.is_some());
    if let Some(failure) = &report.failure {
        if failure.len() > MAX_RUNTIME_REPORT_TEXT_BYTES {
            return Err(ProtocolError::InvalidPayload("media runtime failure"));
        }
        writer.string(failure)?;
    }
    Ok(())
}

fn decode_media_runtime(reader: &mut WireReader<'_>) -> Result<MediaRuntimeReport, ProtocolError> {
    Ok(MediaRuntimeReport {
        active: reader.bool()?,
        playing: reader.bool()?,
        ended: reader.bool()?,
        current_time_100ns: reader.u64()?,
        duration_100ns: reader.u64()?,
        backend: reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES)?,
        mime_type: reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES)?,
        video_codec: reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES)?,
        audio_codec: reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES)?,
        encoded_queue_bytes: reader.u64()?,
        encoded_queue_limit_bytes: reader.u64()?,
        decoded_frame_queue_depth: reader.u16()?,
        decoded_frame_queue_limit: reader.u16()?,
        frames_submitted: reader.u64()?,
        dropped_frames: reader.u64()?,
        width: reader.u32()?,
        height: reader.u32()?,
        failure: reader
            .bool()?
            .then(|| reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES))
            .transpose()?,
    })
}

fn encode_strings(
    writer: &mut WireWriter,
    values: &[String],
    field: &'static str,
) -> Result<(), ProtocolError> {
    if values.len() > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload(field));
    }
    writer.u32(values.len() as u32);
    for value in values {
        if value.len() > MAX_RUNTIME_REPORT_TEXT_BYTES {
            return Err(ProtocolError::InvalidPayload("runtime report text"));
        }
        writer.string(value)?;
    }
    Ok(())
}

fn decode_strings(reader: &mut WireReader<'_>) -> Result<Vec<String>, ProtocolError> {
    let count = reader.u32()? as usize;
    if count > MAX_RUNTIME_REPORT_ENTRIES {
        return Err(ProtocolError::InvalidPayload("runtime report count"));
    }
    (0..count)
        .map(|_| reader.string(MAX_RUNTIME_REPORT_TEXT_BYTES))
        .collect()
}
