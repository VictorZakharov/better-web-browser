use super::*;
use crate::engine::dom;
mod audio_codecs;
mod audio_data;
mod dom_parser;
mod opus_capabilities;
mod serialization;
mod url_pattern;
mod url_resolution;
mod video_codecs;
mod wake_lock;
mod xhr_document;

fn execute_html(html: &str) -> (super::super::dom::Dom, ScriptOutcome) {
    let dom = dom::parse_with_scripting(html, true);
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let outcome = execute(dom.document.clone(), "https://example.com/", &scripts);
    (dom, outcome)
}

mod adjacent_insertion;
mod attributes;
mod beacon;
mod bindings;
mod bitmap_codecs;
mod bitmap_options;
mod bitmap_pixels;
mod bitmap_renderer;
mod cache_storage;
mod canvas;
mod canvas_bitmap;
mod canvas_blend;
mod canvas_filter;
mod canvas_focus_ring;
mod canvas_presentation;
mod canvas_shadow;
mod canvas_stroke_styles;
mod canvas_svg_path;
#[cfg(windows)]
mod canvas_text;
mod canvas_transform;
mod channel_messaging;
mod check_visibility;
mod checkable;
mod collections;
mod compatibility;
mod conditional_query_integration;
mod crypto;
mod csp_events;
mod css_animations;
mod css_transitions;
mod cssom;
mod cssom_layers;
mod cssom_nesting;
mod cssom_owned;
mod cssom_view;
mod cssom_view_scroll;
mod custom_element_provenance;
mod custom_element_scopes;
mod custom_elements;
mod declarative_shadow;
mod declarative_shadow_slots;
mod document_streams;
mod drag_drop;
mod editing_hosts;
mod element_scrolling;
mod embedded_elements;
mod encoding_capabilities;
mod event_handler_attributes;
mod event_source;
mod events;
mod feature_presence;
mod file_api;
mod file_input;
mod focus_selectors;
mod font_loading;
mod form_data_lifecycle;
mod form_length_reflection;
mod form_review_regressions;
mod form_selectors;
mod form_state;
mod form_submission;
mod form_submit_reset;
mod form_temporal_values;
mod form_user_edits;
mod form_validity;
mod forms;
mod fragment_geometry;
mod fragment_navigation;
mod fragment_parsing;
mod fullscreen;
#[cfg(windows)]
mod gamepads;
mod get_html;
mod history;
mod hyperlinks;
mod image_decoders;
mod image_input;
mod inserted_scripts;
mod intersection_observer;
mod media;
mod media_audio_tracks;
mod media_capabilities;
mod media_container_capabilities;
mod media_diagnostics;
mod media_queries;
mod media_queries_idl;
mod media_readiness;
mod media_source_initial;
mod media_source_segments;
mod media_source_tracks;
mod media_track_indexed;
mod media_video_tracks;
mod metadata;
mod meter_progress_numeric;
mod microtasks;
mod modules;
mod mutations;
mod native_editing;
mod native_text_selection;
mod navigator;
mod network;
mod network_body;
mod network_diagnostics;
mod network_images;
mod network_integrity;
mod node_equality;
mod node_standard;
mod nodes;
mod parse_html_unsafe;
mod performance;
mod point_queries;
mod pointer_hover;
mod pointer_lock;
mod popover;
mod private_bridge;
mod published_geometry;
mod ranges;
mod request_state;
mod resize_observer;
mod scoped_invalidation;
mod selectors;
mod shadow_cloning;
mod shadow_dom;
mod shadow_focus;
mod shadow_part_idl;
mod storage_event;
mod storage_manager;
mod streams_bytes;
mod streams_compression;
mod streams_encoding;
mod streams_pipe;
mod streams_transform;
mod style_declaration;
mod svg;
mod table_geometry;
mod tasks;
mod template_inertness;
mod template_shadow_reflection;
mod text_control_selection;
mod text_control_selection_snapshots;
mod text_encoding;
mod text_tracks;
mod timer_diagnostics;
mod traversal;
mod video_frames;
mod web_animations;
mod web_animations_easing;
mod web_animations_interfaces;
mod web_animations_keyframes;
mod web_animations_options;
mod web_animations_playback;
mod web_animations_replacement;
mod web_audio;
mod web_audio_analyser;
mod web_audio_automation;
mod web_audio_biquad;
mod web_audio_buffer_acquisition;
mod web_audio_buffer_copies;
mod web_audio_buffer_idl;
mod web_audio_capture;
mod web_audio_channel_constraints;
mod web_audio_channel_modes;
mod web_audio_channel_options;
mod web_audio_channels;
mod web_audio_compressor;
mod web_audio_connections;
mod web_audio_convolver;
mod web_audio_decode;
mod web_audio_delay;
mod web_audio_delay_layouts;
mod web_audio_enum_conversion;
mod web_audio_feedback;
mod web_audio_feedback_edits;
mod web_audio_filter_layouts;
mod web_audio_iir;
mod web_audio_interfaces;
mod web_audio_live;
mod web_audio_modulation;
mod web_audio_periodic_wave;
mod web_audio_processor_channels;
mod web_audio_resources;
mod web_audio_routing_fixture;
mod web_audio_routing_resources;
mod web_audio_script_blocks;
mod web_audio_script_lifecycle;
mod web_audio_script_live;
mod web_audio_script_pcm;
mod web_audio_script_processor;
mod web_audio_spatial;
mod web_audio_spatial_options;
mod web_audio_speaker_matrix;
mod web_audio_suspend;
mod web_audio_waveshaper;
mod web_audio_waveshaper_oversample;
#[cfg(windows)]
mod webgl;
#[cfg(windows)]
mod webgl_argument_brands;
mod webgl_attributes;
#[cfg(windows)]
mod webgl_contracts;
mod webgl_float_sampling;
mod webgl_float_textures;
#[cfg(windows)]
mod webgl_fragment_depth;
#[cfg(windows)]
mod webgl_instancing;
mod webgl_khronos_reporter;
#[cfg(windows)]
mod webgl_lifecycle;
#[cfg(windows)]
mod webgl_numeric_arguments;
#[cfg(windows)]
mod webgl_numeric_lists;
#[cfg(windows)]
mod webgl_presentation;
#[cfg(windows)]
mod webgl_shader_extensions;
#[cfg(windows)]
mod webgl_shader_validation;
mod webgl_stencil_masks;
#[cfg(windows)]
mod webgl_texture_lod;
mod webgl_uniform_reflection;
#[cfg(windows)]
mod webgl_vertex_arrays;
mod websocket;
mod window_named_access;
mod workers;
mod xhr_reuse;
