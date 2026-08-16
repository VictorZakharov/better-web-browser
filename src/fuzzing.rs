//! Shared, deterministic entry points for libFuzzer and stable corpus replay.
//!
//! The public surface is intentionally small: fuzz binaries contain no parser logic, so every
//! minimized finding can be replayed by ordinary `cargo test` on Windows.

use crate::engine::css::StyleSet;
use crate::engine::dom::{self, Node};
use crate::engine::page::Page;
use crate::limits::{
    MAX_CSS_SOURCE_BYTES, MAX_HTML_INPUT_BYTES, MAX_URL_BYTES, bounded_utf8_prefix,
};
use crate::navigation::{normalize_user_input, resolve_url};

const MAX_MUTATION_OPERATIONS: usize = 512;
const MAX_HOST_BINDING_OPERATIONS: usize = 256;

pub fn html_document(data: &[u8]) {
    let html = lossy_input(data, MAX_HTML_INPUT_BYTES);
    let _ = dom::parse(&html);
}

pub fn html_fragment(data: &[u8]) {
    let html = lossy_input(data, MAX_HTML_INPUT_BYTES);
    let dom = dom::parse("<main id='target'></main>");
    let target = dom.elements_named("main").next().unwrap();
    Node::replace_inner_html(&target, &html, true);
}

pub fn css_stylesheet(data: &[u8]) {
    let css = lossy_input(data, MAX_CSS_SOURCE_BYTES);
    let dom = dom::parse("<main class='target'><span>text</span></main>");
    let _ = StyleSet::from_dom(&dom, &[css], 1024.0);
}

pub fn url_resolution(data: &[u8]) {
    let input = lossy_input(data, MAX_URL_BYTES);
    let split = input
        .char_indices()
        .nth(input.chars().count() / 2)
        .map_or(input.len(), |(index, _)| index);
    let (base, reference) = input.split_at(split);
    let _ = normalize_user_input(&input);
    let _ = resolve_url(base, reference);
}

pub fn dom_mutations(data: &[u8]) {
    let dom = dom::parse("<main><p>seed</p></main>");
    let root = dom.elements_named("main").next().unwrap();
    let mut nodes = vec![root.clone()];
    for (index, operation) in data.iter().take(MAX_MUTATION_OPERATIONS).enumerate() {
        let selected = usize::from(*operation) % nodes.len();
        let node = nodes[selected].clone();
        match operation % 5 {
            0 => {
                let child = Node::create_element_for(&dom.document, "span");
                if Node::append_child(&node, child.clone()) {
                    nodes.push(child);
                }
            }
            1 => {
                node.set_attr("data-fuzz", &index.to_string());
            }
            2 => Node::set_text_content(&node, &format!("value-{index}")),
            3 if node.id() != root.id() => Node::remove_from_parent(&node),
            _ => {
                let child = Node::create_text_for(&dom.document, "text");
                let _ = Node::append_child(&node, child);
            }
        }
    }
}

pub fn javascript_host_bindings(data: &[u8]) {
    let mut operations = String::new();
    for (index, operation) in data.iter().take(MAX_HOST_BINDING_OPERATIONS).enumerate() {
        match operation % 5 {
            0 => operations.push_str("root.appendChild(document.createElement('span'));"),
            1 => operations.push_str(&format!("root.setAttribute('data-fuzz','{index}');")),
            2 => operations.push_str(&format!("root.textContent='value-{index}';")),
            3 => operations.push_str("root.classList.toggle('active');"),
            _ => operations.push_str("root.cloneNode(true);"),
        }
    }
    let html = format!(
        "<main id='root'></main><script>const root=document.getElementById('root');{operations}</script>"
    );
    let mut page = Page::parse_scripted(&html, "https://example.test/");
    let _ = page.execute_scripts();
}

fn lossy_input(data: &[u8], maximum_output_bytes: usize) -> String {
    let bytes = &data[..data.len().min(maximum_output_bytes)];
    if let Ok(input) = std::str::from_utf8(bytes) {
        return input.to_owned();
    }

    // Invalid bytes expand to the replacement character, so cap the converted representation too.
    let converted = String::from_utf8_lossy(bytes);
    bounded_utf8_prefix(&converted, maximum_output_bytes)
        .0
        .to_owned()
}
