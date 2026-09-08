use super::*;

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 600.0, 1.0, false)
}

fn input(source: &str) -> Rc<SheetInput> {
    Rc::new(SheetInput {
        source: source.into(),
        base_url: "https://example.test/sheet.css".into(),
        scope: RuleScope::Document,
    })
}

fn compile(
    mut inputs: Vec<Rc<SheetInput>>,
    environment: MediaEnvironment,
    previous: Option<&CompiledRules>,
    limit: usize,
) -> CompiledRules {
    let (rules, parsed) = assemble_with_limit(&mut inputs, environment, previous, limit);
    CompiledRules {
        index: RuleIndex::new(&rules),
        rules,
        inputs,
        parsed,
        environment: Some(environment),
    }
}

fn assert_matches_fresh(set: &CompiledRules, limit: usize) {
    let mut fresh = Vec::new();
    let mut order = 0;
    for input in &set.inputs {
        let remaining = limit.saturating_sub(fresh.len());
        parse_stylesheet_with_rule_budget(
            &input.source,
            &input.base_url,
            set.environment.unwrap(),
            &mut order,
            &mut fresh,
            input.scope,
            remaining,
        );
    }
    assert_eq!(format!("{:?}", set.rules), format!("{fresh:?}"));
    assert!(
        set.parsed
            .iter()
            .map(|sheet| sheet.rules.len())
            .sum::<usize>()
            <= limit
    );
}

#[test]
fn changed_source_sets_reuse_unchanged_sheets_and_rebase_every_occurrence() {
    let first = input(".card,.alternate{color:red}");
    let second = input(".card{color:blue}");
    let added = input(".card{color:green}");
    let old = compile(vec![first.clone(), second.clone()], environment(), None, 30);
    let reordered = compile(
        vec![added, second.clone(), first.clone()],
        environment(),
        Some(&old),
        30,
    );
    assert!(Rc::ptr_eq(&old.parsed[0], &reordered.parsed[2]));
    assert!(Rc::ptr_eq(&old.parsed[1], &reordered.parsed[1]));
    assert_matches_fresh(&reordered, 30);

    let edited = input(".card{color:purple}");
    let changed = compile(
        vec![first.clone(), edited],
        environment(),
        Some(&reordered),
        30,
    );
    assert!(Rc::ptr_eq(&old.parsed[0], &changed.parsed[0]));
    assert!(!Rc::ptr_eq(&old.parsed[1], &changed.parsed[1]));
    assert_matches_fresh(&changed, 30);

    let repeated = compile(
        vec![first.clone(), first],
        environment(),
        Some(&changed),
        30,
    );
    assert!(Rc::ptr_eq(&repeated.parsed[0], &repeated.parsed[1]));
    assert_eq!(
        repeated
            .rules
            .iter()
            .map(|rule| rule.order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_matches_fresh(&repeated, 30);
}

#[test]
fn document_collection_keeps_chunks_across_inline_sheet_edits() {
    let dom = dom::parse("<style>p{color:red}</style><style>p{color:blue}</style><p>x</p>");
    let old = collect(&dom.document, "https://example.test/", &[], environment());
    Node::set_text_content(
        &dom.elements_named("style").nth(1).unwrap(),
        "p{color:green}",
    );
    let updated = collect(&dom.document, "https://example.test/", &[], environment());
    assert!(!Rc::ptr_eq(&old, &updated));
    assert!(Rc::ptr_eq(&old.parsed[0], &updated.parsed[0]));
    assert!(!Rc::ptr_eq(&old.parsed[1], &updated.parsed[1]));
    assert_matches_fresh(&updated, MAX_PAGE_CSS_RULES);
}

#[test]
fn expired_temporary_cascade_does_not_hide_an_older_live_parse_source() {
    let dom = dom::parse("<style>p{color:red}</style><style>b{color:blue}</style><p>x</p>");
    let second = dom.elements_named("style").nth(1).unwrap();
    let old = collect(&dom.document, "https://example.test/", &[], environment());
    Node::set_text_content(&second, "b{color:green}");
    let temporary = collect(&dom.document, "https://example.test/", &[], environment());
    let expired = Rc::downgrade(&temporary);
    drop(temporary);
    assert!(expired.upgrade().is_none());

    Node::set_text_content(&second, "b{color:purple}");
    let updated = collect(&dom.document, "https://example.test/", &[], environment());
    assert!(Rc::ptr_eq(&old.parsed[0], &updated.parsed[0]));
    assert_matches_fresh(&updated, MAX_PAGE_CSS_RULES);

    Node::set_text_content(&second, "b{color:blue}");
    let restored = collect(&dom.document, "https://example.test/", &[], environment());
    assert!(
        Rc::ptr_eq(&old, &restored),
        "all live candidates participate in exact lookup"
    );
}

#[test]
fn changed_base_scope_and_media_never_reuse_stale_parsed_rules() {
    let source = "@media(min-width:900px){.card{color:red}} .card{background-image:url(icon.png)}";
    let original = input(source);
    let old = compile(vec![original.clone()], environment(), None, 20);
    let moved_base = Rc::new(SheetInput {
        source: source.into(),
        base_url: "https://other.test/css/".into(),
        scope: RuleScope::Document,
    });
    let scoped = Rc::new(SheetInput {
        source: source.into(),
        base_url: original.base_url.clone(),
        scope: RuleScope::Shadow(Node::create_document().id()),
    });
    for changed in [moved_base, scoped] {
        let rebuilt = compile(vec![changed], environment(), Some(&old), 20);
        assert!(!Rc::ptr_eq(&old.parsed[0], &rebuilt.parsed[0]));
        assert_matches_fresh(&rebuilt, 20);
    }
    let wide = compile(
        vec![original],
        MediaEnvironment::new(1000.0, 600.0, 1.0, false),
        Some(&old),
        20,
    );
    assert!(!Rc::ptr_eq(&old.parsed[0], &wide.parsed[0]));
    assert_eq!(wide.rules.len(), 2);
    assert_matches_fresh(&wide, 20);
}

#[test]
fn a_partially_parsed_sheet_expands_only_when_a_larger_budget_is_available() {
    let first = input(".a{} .b{} .c{}");
    let second = input(".d{} .e{} .f{}");
    let old = compile(vec![first.clone(), second.clone()], environment(), None, 4);
    assert_eq!(old.parsed[1].rules.len(), 1);
    assert_matches_fresh(&old, 4);
    let expanded = compile(vec![second.clone()], environment(), Some(&old), 4);
    assert_eq!(expanded.rules.len(), 3);
    assert!(!Rc::ptr_eq(&old.parsed[1], &expanded.parsed[0]));
    assert_matches_fresh(&expanded, 4);
    let reordered = compile(vec![second, first.clone()], environment(), Some(&old), 4);
    assert_eq!(reordered.parsed[1].rules.len(), 1);
    assert_matches_fresh(&reordered, 4);
    let shrunk = compile(vec![first], environment(), Some(&old), 2);
    assert_eq!(shrunk.rules.len(), 2);
    assert_matches_fresh(&shrunk, 2);
}

#[test]
fn shared_chunks_do_not_outlive_their_last_live_cascade() {
    let source = input("p{color:red}");
    let old = compile(vec![source.clone()], environment(), None, 10);
    let updated = compile(
        vec![source, input("b{color:blue}")],
        environment(),
        Some(&old),
        10,
    );
    let weak = Rc::downgrade(&old.parsed[0]);
    drop(old);
    assert!(weak.upgrade().is_some());
    drop(updated);
    assert!(weak.upgrade().is_none());
}
