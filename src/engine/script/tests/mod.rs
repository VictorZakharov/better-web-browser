use super::*;
use crate::engine::dom;

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

mod attributes;
mod bindings;
mod canvas;
mod channel_messaging;
mod compatibility;
mod crypto;
mod cssom;
mod cssom_view;
mod cssom_view_scroll;
mod custom_elements;
mod embedded_elements;
mod events;
mod forms;
mod fullscreen;
mod hyperlinks;
mod intersection_observer;
mod media;
mod media_diagnostics;
mod media_queries;
mod media_readiness;
mod media_source_initial;
mod media_source_segments;
mod media_source_tracks;
mod metadata;
mod modules;
mod mutations;
mod network;
mod network_body;
mod network_diagnostics;
mod nodes;
mod pointer_hover;
mod published_geometry;
mod ranges;
mod request_state;
mod selectors;
mod shadow_dom;
mod svg;
mod tasks;
mod template_inertness;
mod timer_diagnostics;
mod traversal;
mod workers;
