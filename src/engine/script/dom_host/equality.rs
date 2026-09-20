//! DOM node equality compares data and light-tree children, never authored getters.
use super::*;

pub(super) fn equal(state: &HostState, left: &NodeRef, right: &NodeRef) -> bool {
    let mut pending = vec![(left.clone(), right.clone())];
    while let Some((left, right)) = pending.pop() {
        if left.id() == right.id() {
            continue;
        }
        if node_type(state, Some(&left)) != node_type(state, Some(&right))
            || !same_data(&left.data, &right.data)
        {
            return false;
        }
        let a = left.children.borrow();
        let b = right.children.borrow();
        if a.len() != b.len() {
            return false;
        }
        pending.extend(a.iter().cloned().zip(b.iter().cloned()));
    }
    true
}

// https://dom.spec.whatwg.org/#concept-node-equals
// Template contents and attached shadow trees are not light-tree children.
fn same_data(left: &NodeData, right: &NodeData) -> bool {
    match (left, right) {
        (NodeData::Document, NodeData::Document)
        | (NodeData::ShadowRoot(_), NodeData::ShadowRoot(_)) => true,
        (NodeData::Element(a), NodeData::Element(b)) => {
            if a.name != b.name {
                return false;
            }
            let a = a.attrs.borrow();
            let b = b.attrs.borrow();
            if a.len() != b.len() {
                return false;
            }
            // Attribute order and prefix are not part of Attr equality.
            let b: std::collections::HashMap<_, _> = b
                .iter()
                .map(|attr| ((&attr.name.ns, &attr.name.local), &attr.value))
                .collect();
            a.iter()
                .all(|attr| b.get(&(&attr.name.ns, &attr.name.local)) == Some(&&attr.value))
        }
        (NodeData::Text(a), NodeData::Text(b))
        | (NodeData::Cdata(a), NodeData::Cdata(b))
        | (NodeData::Comment(a), NodeData::Comment(b)) => *a.borrow() == *b.borrow(),
        (
            NodeData::Doctype {
                name: a,
                public_id: ap,
                system_id: asys,
            },
            NodeData::Doctype {
                name: b,
                public_id: bp,
                system_id: bsys,
            },
        ) => a == b && ap == bp && asys == bsys,
        (
            NodeData::ProcessingInstruction {
                target: a,
                contents: ac,
            },
            NodeData::ProcessingInstruction {
                target: b,
                contents: bc,
            },
        ) => a == b && *ac.borrow() == *bc.borrow(),
        _ => false,
    }
}
