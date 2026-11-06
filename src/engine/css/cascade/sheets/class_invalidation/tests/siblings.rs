//! Sibling-combinator uses restyle later siblings or the element's subtree, not its parent's.
use super::*;

fn list(css: &str, target: usize) -> String {
    let items = (0..40)
        .map(|item| {
            let id = if item == target { " id=target" } else { "" };
            format!("<li{id}><p>item <b>{item}</b></p><p class=c>x</p></li>")
        })
        .collect::<String>();
    format!("<style>{css}</style><main><ul>{items}</ul><p class=after>after</p></main>")
}

#[test]
fn sibling_uses_restyle_later_siblings_or_the_subtree_not_the_parent() {
    // The target is near the end, so the parent's subtree is far larger than the reach.
    for css in [
        ".a + li{color:red}",
        ".a ~ li{margin-left:2px}",
        ".a ~ li .c{color:blue}",
        ".a > p + p{color:green}",
        ".a p ~ .c{font-weight:bold}",
        ".a > * + *{color:green}",
    ] {
        let stats = change_class(&list(css, 36), "a");
        assert!(!stats.full_rebuild, "{css}");
        assert!(stats.recomputed_styles < 40, "{css}: {stats:?}");
        assert_eq!(stats.invalidation_fallbacks.sibling_combinator, 1, "{css}");
    }
}

#[test]
fn sibling_scopes_stay_correct() {
    for (css, target) in [
        (".a + li{color:red}", 0),
        (".a ~ li b{color:red}", 3),
        (".a + li + li > p{color:red}", 10),
        (".a ~ li:last-child{display:none}", 20),
        (".a .c ~ b{color:red}", 5),
        ("li:not(.a) + li{color:red}", 7),
        (".a ~ * .c{color:red} li + li{margin:1px}", 39),
        (".a + li, .a ~ .after{color:red}", 39),
    ] {
        change_class(&list(css, target), "a");
    }
}

#[test]
fn sibling_rules_below_a_changed_container_restyle_their_subjects_only() {
    // MediaWiki's collapsible tables: a class on the table, then rows after the first.
    let rows = (0..60)
        .map(|row| format!("<tr><td><a href=#>link {row}</a> <b>{row}</b></td><td>x</td></tr>"))
        .collect::<String>();
    for css in [
        ".a > tbody > tr + tr{display:none}",
        ".a tr ~ tr .note{color:red}",
        ".a td + td{color:blue}",
    ] {
        let html = format!("<style>{css}</style><table id=target><tbody>{rows}</tbody></table>");
        let stats = change_class(&html, "a");
        assert!(
            stats.recomputed_styles < stats.total_styles / 2,
            "{css}: {stats:?}"
        );
    }
}
