//! Selectors Level 4 mutability states for HTML controls and editing hosts.
use super::*;

pub(super) fn matches_read_write(node: &NodeRef) -> bool {
    if matches!(node.tag_name(), Some("input" | "textarea")) {
        if node.attr_ref("readonly").is_some() || super::is_disabled(node) {
            return false;
        }
        if node.tag_name() == Some("textarea") {
            return true;
        }
        let input_type = node.attr("type").unwrap_or_default();
        return matches!(
            input_type.to_ascii_lowercase().as_str(),
            "" | "text"
                | "search"
                | "tel"
                | "url"
                | "email"
                | "password"
                | "number"
                | "date"
                | "month"
                | "week"
                | "time"
                | "datetime-local"
        );
    }
    let mut current = Some(node.clone());
    while let Some(element) = current {
        if let Some(value) = element.attr_ref("contenteditable") {
            let value = value.to_ascii_lowercase();
            if value.is_empty() || value == "true" || value == "plaintext-only" {
                return true;
            }
            if value == "false" {
                return false;
            }
        }
        current = element.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_hosts_and_text_controls_match_mutability_selectors() {
        let dom = dom::parse(
            "<div contenteditable><span id=editable>text</span><i contenteditable=false id=locked></i></div>\
             <input id=input><input id=readonly readonly><textarea id=textarea></textarea>\
             <button id=button></button><p id=plain></p>",
        );
        for (name, expected) in [
            ("editable", true),
            ("locked", false),
            ("input", true),
            ("readonly", false),
            ("textarea", true),
            ("button", false),
            ("plain", false),
        ] {
            let node = dom::Node::descendants(&dom.document)
                .find(|node| node.attr("id").as_deref() == Some(name))
                .unwrap();
            assert_eq!(matches_read_write(&node), expected, "{name}");
            let writable = parse_selector(":read-write").unwrap();
            let readonly = parse_selector(":read-only").unwrap();
            assert_eq!(selector_matches(&writable, &node), expected);
            assert_eq!(selector_matches(&readonly, &node), !expected);
        }
    }
}
