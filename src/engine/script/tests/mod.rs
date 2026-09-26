use super::*;
use crate::engine::dom;
mod dom_parser;
mod serialization;
mod url_resolution;
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
mod bindings;
mod canvas;
mod canvas_bitmap;
mod canvas_blend;
mod canvas_filter;
mod canvas_focus_ring;
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
mod cssom;
mod cssom_layers;
mod cssom_nesting;
mod cssom_owned;
mod cssom_view;
mod cssom_view_scroll;
mod custom_elements;
mod document_streams;
mod drag_drop;
mod editing_hosts;
mod element_scrolling;
mod embedded_elements;
mod event_handler_attributes;
mod event_source;
mod events;
mod feature_presence;
mod file_input;
mod focus_selectors;
mod font_loading;
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
mod hyperlinks;
mod image_input;
mod inserted_scripts;
mod intersection_observer;
mod media;
mod media_audio_tracks;
mod media_diagnostics;
mod media_queries;
mod media_readiness;
mod media_source_initial;
mod media_source_segments;
mod media_source_tracks;
mod media_track_indexed;
mod media_video_tracks;
mod metadata;
mod microtasks;
mod modules;
mod mutations;
mod native_editing;
mod navigator;
mod network;
mod network_body;
mod network_diagnostics;
mod network_images;
mod network_integrity;
mod node_equality;
mod node_standard;
mod nodes;
mod performance;
mod point_queries;
mod pointer_hover;
mod pointer_lock;
mod published_geometry;
mod ranges;
mod request_state;
mod resize_observer;
mod scoped_invalidation;
mod selectors;
mod shadow_dom;
mod storage_event;
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
mod text_encoding;
mod text_tracks;
mod timer_diagnostics;
mod traversal;
mod web_animations;
mod web_animations_easing;
mod web_animations_interfaces;
mod web_animations_keyframes;
mod web_animations_options;
mod web_animations_playback;
mod web_animations_replacement;
mod websocket;
mod workers;
mod xhr_reuse;
