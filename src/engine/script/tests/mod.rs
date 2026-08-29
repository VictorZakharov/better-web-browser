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
mod compatibility;
mod cssom;
mod custom_elements;
mod events;
mod fullscreen;
mod intersection_observer;
mod media_queries;
mod modules;
mod mutations;
mod network;
mod network_body;
mod selectors;
mod shadow_dom;
mod tasks;
mod workers;
