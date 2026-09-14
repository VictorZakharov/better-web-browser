use super::*;
use crate::engine::{FontSpec, Page, TextMeasurer, layout_geometry_with_style_viewport};

struct Measurer;
impl TextMeasurer for Measurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.len() as f32 * font.size / 2.0, font.size)
    }
}

fn run_with_geometry(html: &str) -> Vec<crate::engine::css::StyleRefreshStats> {
    let page = Rc::new(RefCell::new(Page::parse_scripted(
        html,
        "https://example.test/",
    )));
    let dom = page.borrow().dom.clone();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&observed);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    runtime.set_layout_flush_callback(Box::new(move |invalidation, _| {
        let mut page = page.borrow_mut();
        let stats =
            page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, invalidation);
        let fresh = page.style_for_viewport(800.0, 600.0);
        // A geometry-only cache defers display:none descendants. Compare every retained
        // entry, then verify observable geometry below; do not unwind through a V8 callback.
        let correct = page
            .cached_style_for_viewport(800.0, 600.0)
            .unwrap()
            .styles
            .iter()
            .all(|(id, style)| fresh.styles.get(id) == Some(style));
        captured.borrow_mut().push((stats, correct));
        Some(
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut Measurer)
                .node_bounds,
        )
    }));
    let node = dom.elements_named("script").last().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.test/#move".into(),
        code: node.text_content(),
        node,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    drop(runtime);
    Rc::try_unwrap(observed)
        .unwrap()
        .into_inner()
        .into_iter()
        .map(|(stats, correct)| {
            assert!(correct, "incremental styles differ from the fresh cascade");
            stats
        })
        .collect()
}

#[test]
fn script_tree_changes_refresh_structural_selectors_and_synchronous_geometry() {
    for removal in [
        "loader.remove()",
        "pane.removeChild(loader)",
        "document.adoptNode(loader)",
    ] {
        run_with_geometry(&format!(
            "<style>b{{display:block;width:200px}}b:first-child{{width:100px}}</style>
            <section id=pane><b id=target>x</b></section><script>
            const pane=document.getElementById('pane'), target=document.getElementById('target');
            const widths=[target.getBoundingClientRect().width];
            const loader=document.createElement('script'); loader.type='application/json';
            pane.insertBefore(loader, target); widths.push(target.getBoundingClientRect().width);
            {removal}; widths.push(target.getBoundingClientRect().width);
            if(widths.join(',') !== '100,200,100') throw new Error(widths.join(','));</script>"
        ));
    }
}

#[test]
fn equivalent_stylesheet_replacement_preserves_small_synchronous_refreshes() {
    let stats = run_with_geometry(&format!(
        "<style id=sheet>section{{color:red}} b{{display:block;width:100px}} .wide{{width:200px}}</style>
        <section><b id=target>x</b></section><aside>{}</aside><script>
        const target=document.getElementById('target'), sheet=document.getElementById('sheet');
        target.getBoundingClientRect();
        sheet.textContent=sheet.textContent; target.className='wide';
        if(target.getBoundingClientRect().width !== 200) throw new Error('stale geometry');
        sheet.textContent='b{{display:block;width:300px}}';
        if(target.getBoundingClientRect().width !== 300) throw new Error('stale rules');</script>",
        "<p>unrelated</p>".repeat(500)
    ));
    assert_eq!(stats.len(), 3);
    assert!(!stats[1].full_rebuild);
    assert!(stats[1].recomputed_styles < 10, "{:?}", stats[1]);
    assert!(stats[2].full_rebuild);
}

#[test]
fn moving_between_connected_parents_updates_inheritance_and_siblings_on_both_sides() {
    run_with_geometry(
        "<style>#left{color:red}#right{color:blue}b{display:block;width:200px}b:first-child{width:100px}</style>
        <section id=left><b id=moving>x</b><b id=remaining>y</b></section><section id=right><b>z</b></section>
        <script>const left=document.getElementById('left'), right=document.getElementById('right');
        const moving=document.getElementById('moving'), remaining=document.getElementById('remaining');
        if(moving.getBoundingClientRect().width !== 100 || remaining.getBoundingClientRect().width !== 200) throw new Error('initial');
        right.appendChild(moving);
        if(moving.getBoundingClientRect().width !== 200 || remaining.getBoundingClientRect().width !== 100) throw new Error('moved siblings');
        if(getComputedStyle(moving).color !== 'rgb(0, 0, 255)') throw new Error('inheritance');</script>"
    );
}

#[test]
fn moving_stylesheet_text_to_a_detached_holder_rebuilds_the_connected_rules() {
    run_with_geometry(
        "<style>b{display:block;width:200px}</style><style id=sheet>b{width:100px}</style><b id=target>x</b>
        <script>const target=document.getElementById('target'), sheet=document.getElementById('sheet');
        if(target.getBoundingClientRect().width !== 100) throw new Error('initial');
        document.createElement('div').appendChild(sheet.firstChild);
        if(target.getBoundingClientRect().width !== 200) throw new Error('stale removed stylesheet');</script>"
    );
}

#[test]
fn detached_node_moves_do_not_dirty_the_rendered_document() {
    let (_, outcome) = execute_html(
        "<body><script>const a=document.createElement('div'), b=document.createElement('div');
        a.appendChild(document.createElement('span')); b.appendChild(a.firstChild);</script></body>",
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(!outcome.render_requested);
    assert!(
        outcome.invalidation.roots.is_empty(),
        "{:?}",
        outcome.invalidation
    );
}

#[test]
fn fragment_insertion_preserves_only_the_connected_dirty_root() {
    for operation in [
        "target.appendChild(fragment)",
        "target.insertBefore(fragment, target.firstChild)",
    ] {
        let (dom, outcome) = execute_html(&format!(
            "<body><section id=target><b>old</b></section><aside>unrelated</aside><script>
            const target=document.getElementById('target'), fragment=document.createDocumentFragment();
            fragment.appendChild(document.createElement('span')); {operation};</script></body>"
        ));
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let target = dom.elements_named("section").next().unwrap();
        assert_eq!(outcome.invalidation.roots, vec![target.id()], "{operation}");
        assert!(outcome.render_requested);
    }
}

#[test]
fn moving_a_connected_child_to_a_detached_parent_requests_removal_rendering() {
    for operation in [
        "holder.appendChild(child)",
        "holder.insertBefore(child, null)",
    ] {
        let (dom, outcome) = execute_html(&format!(
            "<body><section><span id=child>moved</span><b>remaining</b></section><script>
            const holder=document.createElement('div'), child=document.getElementById('child');
            {operation};</script></body>"
        ));
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let parent = dom.elements_named("section").next().unwrap();
        assert!(outcome.render_requested, "{operation}");
        assert!(outcome.invalidation.impact.affects_style());
        assert!(outcome.invalidation.impact.affects_layout());
        assert_eq!(outcome.invalidation.roots, vec![parent.id()]);
        assert!(!outcome.invalidation.removed_nodes.is_empty());
    }
}
