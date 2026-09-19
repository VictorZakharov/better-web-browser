use super::*;

#[test]
fn child_effects_never_use_the_top_level_browser_identity() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f=document.createElement('iframe');document.body.append(f);
    </script>"#,
    );
    let result = evaluate(
        &mut runtime,
        &dom,
        r#"
        f.contentWindow.eval(`
            document.cookie='child=value';
            localStorage.setItem('child','value');
            const video=document.createElement('video');document.body.append(video);
            video.play().catch(e=>window.mediaError=e.name);
            document.body.requestFullscreen().catch(e=>window.fullscreenError=e.name);
        `);
    "#,
    );
    assert!(result.cookie_updates.is_empty());
    assert!(result.storage_updates.is_empty());
    assert!(result.media_actions.is_empty());
    assert!(result.fullscreen_actions.is_empty());
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        r#"
        if(f.contentDocument.fullscreenEnabled || f.contentWindow.fullscreenError!=='TypeError' || f.contentWindow.mediaError!=='NotSupportedError') throw Error('unsupported effects must settle');
        if(document.cookie || localStorage.getItem('child')) throw Error('child state reached parent');
    "#,
    );
}
