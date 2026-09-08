use super::*;

#[test]
fn native_hover_updates_css_and_boundary_events_without_reentering_common_ancestors() {
    let dom = dom::parse_with_scripting(
        r#"<style>
      #panel {color:red} #panel:hover {color:green}
    </style><main id=panel><span id=a>A</span><span id=b>B</span></main><output></output>
    <script>
      const panel=document.getElementById('panel'), a=document.getElementById('a');
      const result=document.querySelector('output');
      let enters=0, leaves=0, overs=0;
      panel.onmouseenter=e => { enters++; result.setAttribute('enters', enters);
        result.setAttribute('trusted', e.isTrusted); result.setAttribute('bubbles', e.bubbles);
        result.setAttribute('color', getComputedStyle(panel).color); };
      panel.onmouseleave=e => { leaves++; result.setAttribute('leaves', leaves); };
      panel.onmouseover=e => { overs++; result.setAttribute('overs', overs); };
      panel.onpointerenter=e => result.setAttribute('pointer-enter', e.pointerType);
      a.onmouseout=e => result.setAttribute('related', e.relatedTarget?.id || 'none');
    </script>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: script.text_content(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let panel = dom.elements_named("main").next().unwrap();
    let a = dom.elements_named("span").next().unwrap();
    let b = dom.elements_named("span").nth(1).unwrap();
    let output = dom.elements_named("output").next().unwrap();
    for node in [Some(a.clone()), Some(a.clone()), Some(b.clone()), None] {
        let result = runtime.dispatch_user_input(UserInputEvent::Pointer {
            phase: if node.is_some() { "move" } else { "leave" },
            target: node,
            button: 0,
            buttons: 0,
            x: 10.0,
            y: 10.0,
            activate: false,
            modifiers: UserInputModifiers::default(),
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
    }
    assert_eq!(output.attr("enters").as_deref(), Some("1"));
    assert_eq!(output.attr("leaves").as_deref(), Some("1"));
    assert_eq!(output.attr("overs").as_deref(), Some("2"));
    assert_eq!(output.attr("related").as_deref(), Some("b"));
    assert_eq!(output.attr("trusted").as_deref(), Some("true"));
    assert_eq!(output.attr("pointer-enter").as_deref(), Some("mouse"));
    assert_eq!(output.attr("bubbles").as_deref(), Some("false"));
    assert_eq!(output.attr("color").as_deref(), Some("rgb(0, 128, 0)"));
    assert!(!panel.is_hovered() && !a.is_hovered() && !b.is_hovered());
}

#[test]
fn boundary_related_targets_are_retargeted_and_internal_moves_stay_inside_shadow_root() {
    let dom = dom::parse_with_scripting("<div id=host></div><output></output>", true);
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let script = dom.document.clone();
    let outcome = runtime.execute_initial(&[ScriptInput {
        source_url: "https://example.com/".into(),
        code: r#"
          const hostElement=document.getElementById('host');
          const root=hostElement.attachShadow({mode:'closed'});
          root.innerHTML='<span id=a>A</span><span id=b>B</span>';
          const a=root.getElementById('a'), b=root.getElementById('b');
          const output=document.querySelector('output');
          let outer=0, inner=0;
          document.addEventListener('mouseout',()=>outer++);
          a.addEventListener('mouseout',e=>{
            inner++; output.setAttribute('inner-related',e.relatedTarget.id);
          });
          a.dispatchEvent(new MouseEvent('mouseout',{
            bubbles:true,composed:true,relatedTarget:b
          }));
          output.setAttribute('outer',outer);
          output.setAttribute('inner',inner);
          output.setAttribute('synthetic-hover',hostElement.matches(':hover'));
          b.addEventListener('mouseover',e=>output.setAttribute('entry-related',e.relatedTarget.id));
          document.addEventListener('mouseover',e=>output.setAttribute('outer-target',e.target.id));
          b.dispatchEvent(new MouseEvent('mouseover',{
            bubbles:true,composed:true,relatedTarget:output
          }));
        "#.into(),
        node: script,
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let output = dom.elements_named("output").next().unwrap();
    assert_eq!(output.attr("outer").as_deref(), Some("0"));
    assert_eq!(output.attr("inner").as_deref(), Some("1"));
    assert_eq!(output.attr("inner-related").as_deref(), Some("b"));
    assert_eq!(output.attr("outer-target").as_deref(), Some("host"));
    assert_eq!(output.attr("synthetic-hover").as_deref(), Some("false"));
}
