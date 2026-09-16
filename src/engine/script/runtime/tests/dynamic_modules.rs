use super::*;
use crate::engine::{ScriptFetchOptions, ScriptKind};

fn start(code: &str) -> (dom::Dom, ScriptRuntime) {
    start_with_options(code, ScriptFetchOptions::for_kind(ScriptKind::Classic))
}
fn start_with_options(code: &str, options: ScriptFetchOptions) -> (dom::Dom, ScriptRuntime) {
    let dom = dom::parse_with_scripting("<head></head><body><script></script></body>", true);
    let node = dom.elements_named("script").next().unwrap();
    let code = format!(
        "window.log=[]; window.record=x=>{{log.push(x);document.body.setAttribute('data-log',log.join('|'))}}; {code}"
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let mut script = input(&node, "app/start.js", &code, false);
    script.fetch_options = options;
    let outcome = runtime.execute_initial_before_document_completion(&[script], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, runtime)
}
fn log(dom: &dom::Dom) -> String {
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-log")
        .unwrap_or_default()
}
fn tick(runtime: &mut ScriptRuntime) {
    let outcome = runtime.advance_time(Duration::ZERO, 1);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
fn finish(runtime: &mut ScriptRuntime, url: &str, source: &str) {
    let url = format!("https://example.com/{url}");
    runtime.complete_module_fetch(url.clone(), Ok((url, source.into())));
}

#[test]
fn dynamic_import_fetches_once_and_returns_one_namespace_after_evaluation() {
    let (dom, mut runtime) = start(
        "window.a=import('./shared.js');window.b=import('./shared.js');Promise.all([a,b]).then(([x,y])=>record((x===y)+':'+x.answer+':'+runs));record('caller');",
    );
    let requests = runtime.take_module_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "https://example.com/app/shared.js");
    assert_eq!(log(&dom), "caller");
    assert!(!runtime.has_runnable_dynamic_scripts());
    finish(
        &mut runtime,
        "app/shared.js",
        "globalThis.runs=(globalThis.runs||0)+1;export const answer=42;",
    );
    assert!(runtime.take_module_requests().is_empty());
    assert_eq!(log(&dom), "caller", "preparation cannot evaluate modules");
    tick(&mut runtime);
    tick(&mut runtime);
    assert_eq!(log(&dom), "caller|true:42:1");
}

#[test]
fn dynamic_import_transitive_graph_and_top_level_await_do_not_block_timers() {
    let (dom, mut runtime) = start(
        "Promise.all([import('./root.js'),import('./root.js')]).then(([a,b])=>record((a===b)+':'+a.answer));setTimeout(()=>record('timer'),5);",
    );
    runtime.take_module_requests();
    finish(
        &mut runtime,
        "app/root.js",
        "import {answer} from './dep.js';await new Promise(r=>setTimeout(r,20));export {answer};",
    );
    assert_eq!(
        runtime.take_module_requests()[0].0,
        "https://example.com/app/dep.js"
    );
    assert!(
        runtime
            .advance_time(Duration::from_millis(5), 1)
            .errors
            .is_empty()
    );
    assert_eq!(log(&dom), "timer");
    finish(&mut runtime, "app/dep.js", "export const answer=42;");
    runtime.take_module_requests();
    tick(&mut runtime);
    tick(&mut runtime);
    assert_eq!(log(&dom), "timer");
    assert!(
        runtime
            .advance_time(Duration::from_millis(20), 1)
            .errors
            .is_empty()
    );
    assert_eq!(log(&dom), "timer|true:42");
}

#[test]
fn dynamic_import_preserves_thrown_values_and_cached_parse_errors() {
    for (source, expected) in [("throw 123;", "123"), ("export const = 1;", "SyntaxError")] {
        let (dom, mut runtime) = start(
            "Promise.all([import('./bad.js').catch(e=>e),import('./bad.js').catch(e=>e)]).then(([a,b])=>record((a===b)+':'+(a.name||a)));",
        );
        runtime.take_module_requests();
        finish(&mut runtime, "app/bad.js", source);
        runtime.take_module_requests();
        tick(&mut runtime);
        tick(&mut runtime);
        assert_eq!(log(&dom), format!("true:{expected}"));
    }
}

#[test]
fn inserted_inline_modules_run_asynchronously_with_distinct_records_and_real_base() {
    let (dom, mut runtime) = start(
        r#"
      for(let i=0;i<2;i++){const s=document.createElement('script');s.type='module';
        s.textContent="record(import.meta.url+':'+(document.currentScript===null))";
        s.onload=()=>record('load');document.head.append(s);}
      record('caller');
    "#,
    );
    assert_eq!(log(&dom), "caller");
    assert!(runtime.take_module_requests().is_empty());
    tick(&mut runtime);
    tick(&mut runtime);
    assert_eq!(
        log(&dom),
        "caller|https://example.com/:true|https://example.com/:true"
    );
}

#[test]
fn inserted_module_and_classic_scripts_share_the_explicit_ordered_queue() {
    let (dom, mut runtime) = start(
        r#"
      for(const [type,url] of [['module','first.js'],['','second.js']]){
        const s=document.createElement('script');s.type=type;s.async=false;s.src=url;document.head.append(s);}
    "#,
    );
    let classic = runtime.take_dynamic_script_requests().pop().unwrap();
    runtime.complete_dynamic_script(classic.node, Ok("record('classic')".into()));
    assert!(!runtime.has_ready_dynamic_scripts());
    assert_eq!(
        runtime.take_module_requests()[0].0,
        "https://example.com/first.js"
    );
    finish(
        &mut runtime,
        "first.js",
        "record('module');await new Promise(()=>{});",
    );
    runtime.take_module_requests();
    tick(&mut runtime);
    assert_eq!(log(&dom), "module|classic");
}

#[test]
fn dynamic_import_fetch_failure_rejects_without_unhandled_script_error() {
    let (dom, mut runtime) = start("import('./missing.js').catch(e=>record(e.name));");
    runtime.take_module_requests();
    runtime.complete_module_fetch(
        "https://example.com/app/missing.js".into(),
        Err("HTTP 404".into()),
    );
    runtime.take_module_requests();
    tick(&mut runtime);
    assert_eq!(log(&dom), "TypeError");
}

#[test]
fn dynamic_import_cancellation_discards_the_old_document_and_completion() {
    let (dom, mut runtime) = start("import('./later.js').then(()=>record('wrong'));");
    runtime.take_module_requests();
    runtime.cancel_document();
    finish(&mut runtime, "app/later.js", "record('wrong');");
    assert!(runtime.take_module_requests().is_empty());
    assert_eq!(log(&dom), "");
}

#[test]
fn dynamic_import_retries_failed_fetches_but_deduplicates_current_waiters() {
    let (dom, mut runtime) = start(
        "Promise.all([import('./retry.js').catch(e=>e.name),import('./retry.js').catch(e=>e.name)]).then(x=>{record(x.join(','));return import('./retry.js')}).then(m=>record(m.ok));",
    );
    assert_eq!(runtime.take_module_requests().len(), 1);
    runtime.complete_module_fetch(
        "https://example.com/app/retry.js".into(),
        Err("HTTP 404".into()),
    );
    assert!(
        runtime.has_runnable_dynamic_scripts(),
        "network completion must wake an idle document"
    );
    runtime.take_module_requests();
    tick(&mut runtime);
    tick(&mut runtime);
    assert_eq!(runtime.take_module_requests().len(), 1);
    finish(
        &mut runtime,
        "app/retry.js",
        "export const ok='retry passed'",
    );
    runtime.take_module_requests();
    tick(&mut runtime);
    assert_eq!(log(&dom), "TypeError,TypeError|retry passed");
}

#[test]
fn dynamic_import_credentials_are_module_options_not_classic_transport_defaults() {
    use crate::fetch::{CredentialsMode, RequestMode};
    for (attribute, expected) in [
        (None, CredentialsMode::SameOrigin),
        (Some("anonymous"), CredentialsMode::SameOrigin),
        (Some("use-credentials"), CredentialsMode::Include),
    ] {
        let (_, mut runtime) = start_with_options(
            "import('./credentials.js')",
            ScriptFetchOptions::for_element(ScriptKind::Classic, attribute, Some("no-referrer")),
        );
        let (_, options) = runtime.take_module_requests().pop().unwrap();
        assert_eq!(options.mode, RequestMode::Cors);
        assert_eq!(options.credentials, expected);
        assert_eq!(
            options.referrer_policy,
            crate::fetch::ReferrerPolicy::NoReferrer
        );
    }
}

#[test]
fn inserted_inline_module_freezes_document_base_at_preparation() {
    let (dom, mut runtime) = start(
        r#"
      const b=document.createElement('base');b.href='/assets/';document.head.append(b);
      const s=document.createElement('script');s.type='module';
      s.textContent="import {answer} from './dep.js';record(import.meta.url+':'+answer)";
      document.head.append(s);b.href='/changed/';
    "#,
    );
    assert_eq!(
        runtime.take_module_requests()[0].0,
        "https://example.com/assets/dep.js"
    );
    finish(&mut runtime, "assets/dep.js", "export const answer=42");
    runtime.take_module_requests();
    tick(&mut runtime);
    assert_eq!(log(&dom), "https://example.com/assets/:42");
}

#[test]
fn module_map_caps_distinct_compiled_records_not_only_source_bytes() {
    let (_, mut runtime) = start("");
    for index in 0..crate::limits::MAX_PAGE_SCRIPTS {
        runtime
            .install_module_dependency(&format!("https://example.com/{index}.js"), "")
            .unwrap();
    }
    assert!(
        runtime
            .install_module_dependency("https://example.com/overflow.js", "")
            .is_err()
    );
}

#[test]
fn invalid_inserted_module_url_fires_error_without_sending_a_fetch() {
    let (dom, mut runtime) = start(
        "const s=document.createElement('script');s.type='module';s.src='http://[';s.onerror=()=>record('error');document.head.append(s);record('caller');",
    );
    assert!(runtime.take_module_requests().is_empty());
    assert_eq!(log(&dom), "caller");
    tick(&mut runtime);
    assert_eq!(log(&dom), "caller|error");
}
