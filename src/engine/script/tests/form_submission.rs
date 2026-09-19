use super::*;

fn submit(html: &str) -> ScriptOutcome {
    let (_, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome
}

#[test]
fn direct_submit_skips_validation_and_submit_and_replaces_action_query() {
    let outcome = submit(
        r#"<form action='/find?old=1#anchor'><input name=q required>
        <input name=hidden value=present style='display:none'></form><script>
        const f=document.querySelector('form');
        f.addEventListener('submit',()=>{throw Error('must not dispatch submit')});
        f.addEventListener('invalid',()=>{throw Error('must not validate')},true);
        f.submit();</script>"#,
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/find?q=&hidden=present#anchor")
    );
    assert!(outcome.navigation_options.post.is_none());
}

#[test]
fn request_submit_validates_cancels_and_provides_a_trusted_submitter() {
    let outcome = submit(
        r#"<form><input id=q name=q required><button id=b name=go value=yes>Go</button></form><script>
        const f=document.querySelector('form'), q=document.getElementById('q'), b=document.getElementById('b');
        let invalid=0, submissions=0;
        q.addEventListener('invalid',()=>invalid++);
        f.addEventListener('submit', e=>{
            if(!(e instanceof SubmitEvent) || e.submitter!==b || !e.isTrusted || !e.bubbles || !e.cancelable) throw Error('submit event');
            submissions++; f.requestSubmit(b); e.preventDefault();
        });
        f.requestSubmit(b); if(invalid!==1 || submissions) throw Error('validation');
        q.value='hello'; f.requestSubmit(b); if(submissions!==1) throw Error('reentrancy');
        </script>"#,
    );
    assert!(outcome.navigation_url.is_none());
}

#[test]
fn successful_controls_follow_dom_order_and_submitter_overrides() {
    let outcome = submit(
        r#"<input name=outside value=first form=f><form id=f action='/wrong' method=post>
        <input name=q value=old><input type=checkbox name=c checked><input type=checkbox name=no>
        <fieldset disabled><legend><input name=legend value=yes></legend><input name=disabled value=no></fieldset>
        <select name=s multiple><option value=a selected>A</option><option disabled selected>B</option><option value=c selected>C</option></select>
        <textarea name=t>old</textarea><input type=hidden name=_charset_ value=wrong>
        <button id=b name=go value=chosen formaction='/find?discard=1' formmethod=get>Go</button>
        <button name=no value=no>Other</button><output name=output>not successful</output></form>
        <input name=outside value=last form=f><script>
        const f=document.getElementById('f');
        f.addEventListener('submit',()=>{f.querySelector('[name=q]').value='new value'; f.querySelector('textarea').value='a\nb';});
        f.addEventListener('formdata',e=>e.formData.append('extra','🦀'));
        f.requestSubmit(document.getElementById('b'));</script>"#,
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some(
            "https://example.com/find?outside=first&q=new+value&c=on&legend=yes&s=a&s=c&t=a%0D%0Ab&_charset_=UTF-8&go=chosen&outside=last&extra=%F0%9F%A6%80"
        )
    );
}

#[test]
fn post_encodings_are_carried_as_navigation_bodies() {
    for (encoding, expected) in [
        ("application/x-www-form-urlencoded", "q=a%0D%0Ab&go=yes"),
        ("text/plain", "q=a\r\nb\r\ngo=yes\r\n"),
    ] {
        let outcome = submit(&format!(
            r#"<form action='/post?keep=1' method=post enctype='{encoding}'>
            <textarea name=q>a
b</textarea><button name=go value=yes>Go</button></form><script>
            document.querySelector('button').click();</script>"#
        ));
        assert_eq!(
            outcome.navigation_url.as_deref(),
            Some("https://example.com/post?keep=1")
        );
        let post = outcome.navigation_options.post.unwrap();
        assert_eq!(post.content_type, encoding);
        assert_eq!(String::from_utf8(post.body).unwrap(), expected);
    }
    let outcome = submit(
        r#"<form action='/upload' method=post enctype='multipart/form-data'><input name=q value=hello></form>
        <script>document.querySelector('form').submit()</script>"#,
    );
    let post = outcome.navigation_options.post.unwrap();
    assert!(
        post.content_type
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(
        String::from_utf8(post.body)
            .unwrap()
            .contains("name=\"q\"\r\n\r\nhello\r\n")
    );
}

#[test]
fn invalid_submitters_and_disconnected_forms_do_not_navigate() {
    let outcome = submit(
        r#"<form id=f><button type=button id=wrong>Wrong</button><button id=b>Go</button></form><form id=other></form><script>
        const f=document.getElementById('f'), b=document.getElementById('b');
        for(const call of [()=>f.requestSubmit(document.getElementById('wrong')),()=>f.requestSubmit({})]) {
            let threw=false; try{call()}catch(e){threw=e instanceof TypeError} if(!threw)throw Error('TypeError');
        }
        let threw=false;try{document.getElementById('other').requestSubmit(b)}catch(e){threw=e.name==='NotFoundError'}
        if(!threw)throw Error('NotFoundError');
        f.addEventListener('submit',()=>f.remove()); f.requestSubmit(); f.submit();
        </script>"#,
    );
    assert!(outcome.navigation_url.is_none());
}

#[test]
fn formdata_event_is_mutable_and_reentrant_construction_is_rejected() {
    let outcome = submit(
        r#"<form action='/go'><input name=q value=old></form><script>
        const f=document.querySelector('form'); let calls=0;
        f.addEventListener('formdata',e=>{
            calls++; if(!e.isTrusted || e.cancelable || !(e instanceof FormDataEvent)) throw Error('formdata event');
            e.formData.set('q','new'); f.submit();
            let threw=false; try{new FormData(f)}catch(err){threw=err.name==='InvalidStateError'}
            if(!threw)throw Error('reentrant entry list');
        });
        f.submit(); if(calls!==1)throw Error('recursive submit');</script>"#,
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/go?q=new")
    );
}

#[test]
fn synthetic_anchor_activation_follows_after_cancellation_sensitive_dispatch() {
    let outcome = submit(
        r#"<a id=result href='/wrong'><span>Result</span></a><script>
        const a=document.getElementById('result');
        a.addEventListener('click',e=>{e.preventDefault(); const next=document.createElement('a');next.href='/destination';next.click();});
        a.querySelector('span').click();</script>"#,
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/destination")
    );
    let canceled = submit(
        r#"<a href='/no'>No</a><script>const a=document.querySelector('a');a.onclick=e=>e.preventDefault();a.click()</script>"#,
    );
    assert!(canceled.navigation_url.is_none());
}

#[test]
fn planned_submission_survives_later_disconnection_and_replaces_previous_submission() {
    let canceled = submit(
        r#"<form action='/no'></form><script>
        const f=document.querySelector('form'); f.submit(); queueMicrotask(()=>f.remove());</script>"#,
    );
    assert_eq!(
        canceled.navigation_url.as_deref(),
        Some("https://example.com/no?")
    );
    let latest = submit(
        r#"<form action='/go'><input name=q value=one></form><script>
        const f=document.querySelector('form'); f.submit(); f.querySelector('input').value='two';f.submit();</script>"#,
    );
    assert_eq!(
        latest.navigation_url.as_deref(),
        Some("https://example.com/go?q=two")
    );
}

#[test]
fn input_type_uses_known_states_and_defaults_to_text_without_changing_the_attribute() {
    submit(
        r#"<input><script>
      const input=document.querySelector('input');
      if(input.type!=='text' || input.hasAttribute('type'))throw Error('missing type');
      for(const type of ['text','search','tel','url','email','password','hidden','date','month','week','time','datetime-local','number','range','color','checkbox','radio','file','submit','image','reset','button']) {
        input.type=type.toUpperCase();
        if(input.type!==type || input.getAttribute('type')!==type.toUpperCase())throw Error(type);
      }
      for(const type of ['','bogus',' text ']){input.type=type;if(input.type!=='text')throw Error('invalid type')}
    </script>"#,
    );
}

#[test]
fn optional_submitter_novalidate_and_formdata_disconnection_follow_the_algorithm() {
    let skipped = submit(
        r#"<form action='/skip'><input name=q required><button formnovalidate name=go value=yes>Go</button></form><script>
      const f=document.querySelector('form');
      f.oninvalid=()=>{throw Error('validation was not skipped')};
      f.requestSubmit(document.querySelector('button'));
    </script>"#,
    );
    assert_eq!(
        skipped.navigation_url.as_deref(),
        Some("https://example.com/skip?q=&go=yes")
    );
    let no_button = submit(
        r#"<form action='/none' novalidate><input name=q required><button name=go value=no>Go</button></form><script>
      const f=document.querySelector('form');f.onsubmit=e=>{if(e.submitter!==null)throw Error('submitter must be null')};
      f.requestSubmit();
    </script>"#,
    );
    assert_eq!(
        no_button.navigation_url.as_deref(),
        Some("https://example.com/none?q=")
    );
    let disconnected = submit(
        r#"<form action='/no'></form><script>
      const f=document.querySelector('form'); f.onformdata=()=>f.remove();f.submit();
    </script>"#,
    );
    assert!(disconnected.navigation_url.is_none());
}

#[test]
fn internal_form_navigation_does_not_call_author_timer_or_formdata_replacements() {
    let outcome = submit(
        r#"<form action='/native'><input name=q value=yes></form><script>
      window.FormData=function(){throw Error('author FormData')};
      window.setTimeout=()=>{throw Error('author timer')};
      document.querySelector('form').submit();
    </script>"#,
    );
    assert_eq!(
        outcome.navigation_url.as_deref(),
        Some("https://example.com/native?q=yes")
    );
    let multipart = submit(
        r#"<form action='/native' method=post enctype='multipart/form-data'><input name=q value=yes></form><script>
      window.FormData=function(){throw Error('author FormData')};
      window.clearTimeout=()=>{throw Error('author cancellation')};
      const f=document.querySelector('form');f.submit();f.submit();
    </script>"#,
    );
    let body = String::from_utf8(multipart.navigation_options.post.unwrap().body).unwrap();
    assert!(body.contains("name=\"q\"\r\n\r\nyes\r\n"));
}
