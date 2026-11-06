//! `hidden` changes restyle the row and the rules that name `[hidden]`, not the table body.
use super::*;
use crate::engine::invalidation::record_attribute_change;

/// Toggles `hidden` on the rows with ids in `rows`, refreshes only the recorded attribute
/// changes, and asserts every element matches a from-scratch cascade.
fn toggle(page: &mut Page, css: &str, rows: &[&str], hidden: bool) -> StyleRefreshStats {
    let mut invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("hidden").impact(),
        mutation_count: rows.len(),
        ..RenderInvalidation::default()
    };
    for id in rows {
        let row = Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(*id))
            .expect("row");
        if hidden {
            row.set_attr("hidden", "");
        } else {
            row.remove_attr("hidden");
        }
        record_attribute_change(&mut invalidation.attribute_changes, &row, "hidden");
    }
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}#{}> with {css}, hidden {hidden}",
            node.tag_name().unwrap_or_default(),
            node.attr("id").unwrap_or_default(),
        );
    }
    stats
}

fn table(css: &str) -> Page {
    let rows = (0..40)
        .map(|row| format!("<tr id=r{row}><td><a href=#>cell {row}</a></td><td><b>x</b></td></tr>"))
        .collect::<String>();
    let html = format!(
        "<style>{css}</style><table class=navbox><tbody>{rows}</tbody></table><p id=after>after</p>"
    );
    let mut page = Page::parse(&html, "https://example.test/");
    page.refresh_resources(800.0);
    page
}

#[test]
fn hiding_rows_restyles_the_rows_not_the_table_body() {
    for css in [
        "td{padding:1px}",
        "tr{display:table-row} .navbox td{color:red}",
    ] {
        let mut page = table(css);
        let hidden = toggle(&mut page, css, &["r3", "r4", "r5"], true);
        assert!(!hidden.full_rebuild, "{css}");
        assert!(
            hidden.recomputed_styles < hidden.total_styles / 4,
            "{css}: {hidden:?}"
        );
        let shown = toggle(&mut page, css, &["r3", "r4", "r5"], false);
        assert!(
            shown.recomputed_styles < shown.total_styles / 4,
            "{css}: {shown:?}"
        );
    }
}

#[test]
fn rules_that_read_hidden_stay_correct() {
    for css in [
        "[hidden]{display:block;color:red}",
        "tr:not([hidden]) td{color:blue}",
        "[hidden] + tr{color:green}",
        "tr[hidden] ~ tr td{font-weight:bold}",
        "tbody:has(> [hidden]) td{color:purple}",
        "table:has(tr[hidden]) + p{color:orange}",
        "tr{display:flex} td{color:red}",
    ] {
        let mut page = table(css);
        toggle(&mut page, css, &["r0", "r7"], true);
        toggle(&mut page, css, &["r7"], false);
        toggle(&mut page, css, &["r0"], false);
    }
}
