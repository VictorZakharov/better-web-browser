//! Child navigations to XML resources ("navigate to an XML document").
//! https://html.spec.whatwg.org/multipage/document-lifecycle.html#read-xml
//!
//! The response is buffered and built with the namespace-aware XML parser at end of
//! file, into the Document committed when the response head arrived. HTML-namespace
//! `script` elements then run in tree order, before the end of parsing. Unlike a
//! streaming XML parser, a script therefore sees the complete tree; a well-formedness
//! error replaces the tree with a parser-error document and runs no script.
use super::documents::FrameDocument;
use super::*;
use crate::engine::dom::document::xml;
use std::collections::VecDeque;

pub(super) struct XmlParser {
    content_type: String,
    bytes: Vec<u8>,
    ended: bool,
    /// The parsed document's HTML scripts not yet prepared; `None` until EOF is parsed.
    scripts: Option<VecDeque<NodeRef>>,
    natural_size: Option<(f32, f32)>,
}

/// An SVG document's natural size from its root's absolute `width` and `height`, which
/// sizes an embedding object like an SVG image instead of the default 300x150.
/// https://svgwg.org/svg2-draft/coords.html#SizingSVGInCSS
fn svg_natural_size(document: &NodeRef) -> Option<(f32, f32)> {
    let root = document
        .children
        .borrow()
        .iter()
        .find(|node| node.element().is_some())
        .cloned()?;
    if root.tag_name() != Some("svg") || root.namespace_uri() != Some("http://www.w3.org/2000/svg")
    {
        return None;
    }
    let length = |name: &str| {
        let value = root.attr(name)?;
        let value = value.trim();
        let number: f32 = value
            .strip_suffix("px")
            .unwrap_or(value)
            .trim()
            .parse()
            .ok()?;
        (number.is_finite() && number >= 0.0).then_some(number)
    };
    Some((length("width")?, length("height")?))
}

/// The MIME type essence when `content_type` names an XML resource.
/// https://mimesniff.spec.whatwg.org/#xml-mime-type
pub(super) fn xml_mime_type(content_type: &str) -> Option<String> {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let (_, subtype) = essence.split_once('/')?;
    (essence == "text/xml" || essence == "application/xml" || subtype.ends_with("+xml"))
        .then_some(essence)
}

impl XmlParser {
    pub fn new(content_type: &str) -> Self {
        Self {
            content_type: content_type.into(),
            bytes: Vec::new(),
            ended: false,
            scripts: None,
            natural_size: None,
        }
    }

    pub fn push(&mut self, bytes: &[u8], eof: bool) -> Result<(), String> {
        // The navigation fetch already enforces this bound; the buffer keeps its own.
        if self.bytes.len().saturating_add(bytes.len()) > crate::limits::MAX_HTML_INPUT_BYTES {
            return Err("XML document exceeded the document byte limit".into());
        }
        self.bytes.extend_from_slice(bytes);
        self.ended |= eof;
        Ok(())
    }

    pub fn runnable(&self, blocked: bool) -> bool {
        match &self.scripts {
            None => self.ended,
            Some(_) => !blocked,
        }
    }

    /// Builds the buffered response into `document`, returning its scripts in tree order.
    fn parse(&mut self, document: &NodeRef) -> VecDeque<NodeRef> {
        let bytes = std::mem::take(&mut self.bytes);
        let result = xml::encoding::decode(&bytes, Some(&self.content_type))
            .and_then(|source| xml::parse_into(document, &source));
        if let Err(error) = result {
            xml::replace_with_error(document, &error);
            return VecDeque::new();
        }
        if xml_mime_type(&self.content_type).as_deref() == Some("image/svg+xml") {
            self.natural_size = svg_natural_size(document);
        }
        crate::engine::dom::Node::shadow_including_descendants(document)
            .filter(|node| {
                node.tag_name() == Some("script")
                    && node.namespace_uri() == Some("http://www.w3.org/1999/xhtml")
            })
            .collect()
    }
}

impl FrameDocument {
    pub(super) fn advance_xml(&mut self, runtime: &mut ScriptRuntime) -> ScriptOutcome {
        let Some(parser) = &mut self.xml else {
            return ScriptOutcome::default();
        };
        let Some(scripts) = &mut parser.scripts else {
            if !parser.ended {
                return ScriptOutcome::default();
            }
            let document = runtime.host.borrow().document.clone();
            parser.scripts = Some(parser.parse(&document));
            self.natural_size = parser.natural_size;
            let mut outcome = runtime.parser_dom_changed(Vec::new(), None);
            outcome.request_full_render();
            self.update_pending(runtime);
            return outcome;
        };
        if self.queue.blocked() {
            return ScriptOutcome::default();
        }
        if let Some(node) = scripts.pop_front() {
            let outcome = self.prepare_parser_script(runtime, node);
            self.update_pending(runtime);
            return outcome;
        }
        self.xml = None;
        let outcome = self.finish_parsing(runtime);
        self.update_pending(runtime);
        outcome
    }
}

impl ScriptRuntime {
    /// Commits an empty XML Document for a navigation response; its body is parsed at EOF.
    pub(super) fn commit_xml_frame(
        &mut self,
        navigation: &mut FrameNavigation,
        content_type: &str,
        mime: String,
        fetch: u32,
    ) -> Result<Option<NodeId>, String> {
        let document = crate::engine::dom::Node::create_document();
        let frame = FrameDocument::xml(XmlParser::new(content_type), navigation.scripts);
        let Some(id) =
            self.commit_frame_document(navigation, document.clone(), frame, "UTF-8", Some(fetch))?
        else {
            return Ok(None);
        };
        let child = &self.frames.as_ref().unwrap().children[&id];
        let host = child.host.borrow();
        let mut documents = host.documents.borrow_mut();
        documents.html_documents.remove(&document.id().document());
        documents.document_metadata.insert(
            document.id(),
            crate::engine::script::dom_host::DocumentMetadata {
                url: navigation.url.clone(),
                content_type: mime,
                quirks: false,
            },
        );
        Ok(Some(id))
    }
}
