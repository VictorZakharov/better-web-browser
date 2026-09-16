use super::*;

fn check_script(code: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn public_url_requires_an_explicit_base_for_relative_inputs() {
    check_script(
        r#"
      const check=(ok,label)=>{if(!ok)throw Error(label)};
      let error;try {new URL('/path')}catch(e){error=e}
      check(error instanceof TypeError,'relative URL without base must throw');
      check(!URL.canParse('/path') && URL.parse('/path')===null,'static no-base parsing');
      check(new URL('/path','https://example.org/a').href==='https://example.org/path','explicit base');
      error=null;try{new URL('https://example.org/','bad base')}catch(e){error=e}
      check(error instanceof TypeError,'invalid explicit base');
      const absolute=new URL('https://example.org/a');
      error=null;try{absolute.href='/other'}catch(e){error=e}
      check(error instanceof TypeError && absolute.href==='https://example.org/a','href is not relative');
    "#,
    );
}

#[test]
fn xhr_resolves_relative_references_without_calling_an_author_url_constructor() {
    let (_, outcome) = execute_html(
        r#"<base href='/assets/'><script>
      const NativeURL=URL;
      // A conforming replacement exposes the old internal reliance on an implicit base.
      globalThis.URL=class URL extends NativeURL {
        constructor(input,base) {
          if(base===undefined && !/^[a-z][a-z0-9+.-]*:/i.test(String(input)))throw TypeError('Invalid scheme');
          super(input,base);
        }
      };
      const xhr=new XMLHttpRequest();xhr.open('GET','data.json');xhr.send();
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("missing XHR request")
    };
    assert_eq!(request.url.as_str(), "https://example.com/assets/data.json");
}

#[test]
fn request_and_redirect_parse_urls_independently_of_public_constructor_overrides() {
    check_script(
        r#"
      const base=document.createElement('base');base.href='/assets/';document.head.append(base);
      globalThis.URL=class {constructor(){throw Error('author URL must not be called')}};
      if(new Request('data').url!=='https://example.com/assets/data')throw Error('Request base');
      if(Response.redirect('next').headers.get('location')!=='https://example.com/assets/next')throw Error('redirect base');
      base.href='/changed/';
      if(new Request('data').url!=='https://example.com/changed/data')throw Error('live API base');
      if(new Request('data',{referrer:'ref'}).referrer!=='https://example.com/changed/ref')throw Error('referrer base');
    "#,
    );
}

#[test]
fn invalid_xhr_url_throws_syntax_error_without_canceling_the_current_request() {
    check_script(
        r#"
      const xhr=new XMLHttpRequest();xhr.open('GET','/valid');
      let error;try{xhr.open('GET','http://:bad')}catch(e){error=e}
      if(error?.name!=='SyntaxError' || xhr.readyState!==XMLHttpRequest.OPENED)throw Error('invalid open contract');
    "#,
    );
}

#[test]
fn url_conversions_preserve_web_idl_exceptions_and_native_body_serialization() {
    check_script(
        r#"
      const check=(ok,label)=>{if(!ok)throw Error(label)};
      const marker={}; const throwing={toString(){throw marker}};
      for(const run of [()=>new URL(throwing),()=>URL.parse(throwing),()=>URL.canParse(throwing),
        ()=>URL.parse('https://example.org/',throwing),()=>new Request(throwing),
        ()=>Response.redirect(throwing),()=>new XMLHttpRequest().open('GET',throwing)]) {
        let error;try{run()}catch(e){error=e}check(error===marker,'conversion exception');
      }
      for(const run of [()=>new URL(),()=>URL.parse(),()=>URL.canParse(),
        ()=>new URL(Symbol()),()=>URL.canParse(Symbol()),()=>URL.parse(Symbol()),
        ()=>new Request(Symbol()),()=>Response.redirect(),()=>new XMLHttpRequest().open('GET')]) {
        let error;try{run()}catch(e){error=e}check(error instanceof TypeError,'required or symbol');
      }
      check(new URL('https://example.org/\ud800').pathname==='/%EF%BF%BD','USVString');
      const params=new URLSearchParams('a=%FF&&bad=%');
      check(new URLSearchParams(null).size===0 && new URLSearchParams(1).toString()==='1=','constructor union');
      check(params.size===2 && params.get('a')==='\ufffd' && params.get('bad')==='%','forgiving parser');
      params.toString=()=>{throw Error('public stringifier called')};
      globalThis.URLSearchParams=class{constructor(){throw Error('public params called')}};
      const body=new Request('/send',{method:'POST',body:params});
      body.text().then(text=>check(text==='a=%EF%BF%BD&bad=%25','native body serialization'));
      new Response('x=%FF',{headers:{'content-type':'application/x-www-form-urlencoded'}})
        .formData().then(data=>check(data.get('x')==='\ufffd','native body parsing'));
      check(!('__urlInternals' in globalThis),'private bootstrap handoff');
      check(new Request(undefined).url==='https://example.com/undefined','explicit undefined input');
      new Response(new Uint8Array([0xef,0xbb,0xbf,0x3f,0x78,0x3d,0xff]),
        {headers:{'content-type':'application/x-www-form-urlencoded'}}).formData()
        .then(data=>check(data.get('\ufeff?x')==='\ufffd','raw form bytes, BOM and question mark'));
    "#,
    );
}

#[test]
fn worker_url_resolution_uses_worker_settings_not_public_url_or_location() {
    let loader: std::sync::Arc<WorkerSourceLoader> =
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}")));
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.com/jobs/worker.js",
        r#"
          if(URL.canParse('relative') || URL.parse('relative')!==null)throw Error('implicit base');
          const u=new URL('https://example.org/?a=1&b=2');
          const iterator=u.searchParams.entries();iterator.next();
          u.search='?c=3&d=4';
          if(iterator.next().value[0]!=='d')throw Error('live iterator');
          URL=class{constructor(){throw Error('public URL called')}};
          location.href='https://other.example/';
          if(new Request('data').url!=='https://example.com/jobs/data')throw Error('worker base');
          const xhr=new XMLHttpRequest();xhr.open('GET','data');xhr.send();
        "#,
        "",
        ScriptKind::Classic,
        loader,
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let ScriptFetchAction::Start { request, .. } = &outcome.fetch_actions[0] else {
        panic!("missing worker XHR request")
    };
    assert_eq!(request.url.as_str(), "https://example.com/jobs/data");
}

#[test]
fn api_base_uses_first_base_and_falls_back_from_invalid_or_forbidden_schemes() {
    check_script(
        r#"
      const first=document.createElement('base'),second=document.createElement('base');
      second.href='/ignored/';document.head.append(first,second);
      const check=expected=>{if(new Request('data').url!==expected)throw Error('API base')};
      check('https://example.com/ignored/data');
      first.href='http://:bad';check('https://example.com/data');
      first.href='data:text/html,ignored';check('https://example.com/data');
      first.href='/first/';check('https://example.com/first/data');
      first.remove();check('https://example.com/ignored/data');
      second.remove();history.pushState(null,'','/history/page');check('https://example.com/history/data');
    "#,
    );
}
