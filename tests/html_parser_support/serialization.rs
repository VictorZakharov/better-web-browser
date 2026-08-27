use better_web_browser::engine::dom::{NodeData, NodeId, NodeRef};
use std::collections::HashSet;

const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";
const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
const MATHML_NAMESPACE: &str = "http://www.w3.org/1998/Math/MathML";
const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";
const MAX_SERIALIZED_NODES: usize = 100_000;

pub fn serialize_document(document: &NodeRef) -> Result<String, String> {
    if !matches!(&document.data, NodeData::Document) {
        return Err("document serializer received a non-document root".to_string());
    }
    serialize_children(document)
}

pub fn serialize_fragment(context: &NodeRef) -> Result<String, String> {
    let root = context
        .element()
        .and_then(|element| element.template_contents.borrow().clone())
        .unwrap_or_else(|| context.clone());
    serialize_children(&root)
}

fn serialize_children(root: &NodeRef) -> Result<String, String> {
    let mut output = "#document".to_string();
    let mut visited = HashSet::from([root.id()]);
    let mut count = 1;
    let mut stack = root
        .children
        .borrow()
        .iter()
        .rev()
        .cloned()
        .map(|node| WorkItem {
            node,
            depth: 0,
            expected_parent: root.id(),
        })
        .collect::<Vec<_>>();

    while let Some(item) = stack.pop() {
        count += 1;
        if count > MAX_SERIALIZED_NODES {
            return Err(format!("tree exceeds {MAX_SERIALIZED_NODES} nodes"));
        }
        if !visited.insert(item.node.id()) {
            return Err(format!("node {:?} appears more than once", item.node.id()));
        }
        let actual_parent = item
            .node
            .parent()
            .ok_or_else(|| format!("node {:?} has no parent", item.node.id()))?;
        if actual_parent.id() != item.expected_parent {
            return Err(format!(
                "node {:?} points to parent {:?}, expected {:?}",
                item.node.id(),
                actual_parent.id(),
                item.expected_parent
            ));
        }

        append_node_line(&mut output, &item.node, item.depth);
        let children = item.node.children.borrow().clone();
        if let Some(element) = item.node.element() {
            append_attributes(&mut output, element, item.depth + 1);
            if let Some(contents) = element.template_contents.borrow().clone() {
                if !children.is_empty() {
                    return Err(format!(
                        "template node {:?} has direct children",
                        item.node.id()
                    ));
                }
                if !matches!(&contents.data, NodeData::Document) || contents.parent().is_some() {
                    return Err(format!(
                        "template contents {:?} is not a detached document fragment",
                        contents.id()
                    ));
                }
                if !visited.insert(contents.id()) {
                    return Err(format!(
                        "template contents {:?} appears more than once",
                        contents.id()
                    ));
                }
                count += 1;
                if count > MAX_SERIALIZED_NODES {
                    return Err(format!("tree exceeds {MAX_SERIALIZED_NODES} nodes"));
                }
                append_line(&mut output, item.depth + 1, "content");
                push_children(&mut stack, &contents, item.depth + 2);
                continue;
            }
        }
        for child in children.into_iter().rev() {
            stack.push(WorkItem {
                node: child,
                depth: item.depth + 1,
                expected_parent: item.node.id(),
            });
        }
    }
    Ok(output)
}

fn push_children(stack: &mut Vec<WorkItem>, parent: &NodeRef, depth: usize) {
    stack.extend(
        parent
            .children
            .borrow()
            .iter()
            .rev()
            .cloned()
            .map(|node| WorkItem {
                node,
                depth,
                expected_parent: parent.id(),
            }),
    );
}

fn append_node_line(output: &mut String, node: &NodeRef, depth: usize) {
    let description = match &node.data {
        NodeData::Document => "#document".to_string(),
        NodeData::ShadowRoot(_) => "#shadow-root".to_string(),
        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => {
            if public_id.is_empty() && system_id.is_empty() {
                format!("<!DOCTYPE {name}>")
            } else {
                format!("<!DOCTYPE {name} \"{public_id}\" \"{system_id}\">")
            }
        }
        NodeData::Text(text) => format!("\"{}\"", text.borrow()),
        NodeData::Comment(contents) => format!("<!-- {contents} -->"),
        NodeData::ProcessingInstruction { target, contents } => {
            format!("<?{target} {contents}?>")
        }
        NodeData::Element(element) => format!(
            "<{}{}>",
            element_namespace_prefix(element.name.ns.as_ref()),
            element.name.local
        ),
    };
    append_line(output, depth, &description);
}

fn append_attributes(
    output: &mut String,
    element: &better_web_browser::engine::dom::ElementData,
    depth: usize,
) {
    let mut attributes = element
        .attrs
        .borrow()
        .iter()
        .map(|attribute| {
            let name = format!(
                "{}{}",
                attribute_namespace_prefix(attribute.name.ns.as_ref()),
                attribute.name.local
            );
            (name, attribute.value.to_string())
        })
        .collect::<Vec<_>>();
    attributes.sort_by(|left, right| left.0.encode_utf16().cmp(right.0.encode_utf16()));
    for (name, value) in attributes {
        append_line(output, depth, &format!("{name}=\"{value}\""));
    }
}

fn append_line(output: &mut String, depth: usize, description: &str) {
    output.push_str("\n| ");
    output.push_str(&"  ".repeat(depth));
    output.push_str(description);
}

fn element_namespace_prefix(namespace: &str) -> &'static str {
    match namespace {
        HTML_NAMESPACE => "",
        SVG_NAMESPACE => "svg ",
        MATHML_NAMESPACE => "math ",
        _ => "unknown ",
    }
}

fn attribute_namespace_prefix(namespace: &str) -> &'static str {
    match namespace {
        "" => "",
        XLINK_NAMESPACE => "xlink ",
        XML_NAMESPACE => "xml ",
        XMLNS_NAMESPACE => "xmlns ",
        _ => "unknown ",
    }
}

struct WorkItem {
    node: NodeRef,
    depth: usize,
    expected_parent: NodeId,
}
