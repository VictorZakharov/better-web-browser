use super::*;
use crate::engine::script::ScriptWorkerAction;

#[test]
fn workers_keep_document_identity_and_child_removal_cancels_only_its_workers() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const rootWorker=new Worker('/root.js');window.rootMessage='';
        rootWorker.onmessage=e=>rootMessage=e.data;
        const f=document.createElement('iframe');
        f.srcdoc='<script>window.worker=new Worker("/child.js");window.message="";worker.onmessage=e=>message=e.data;<\/script>';
        document.body.append(f);
    </script>"#,
    );
    let mut child_id = None;
    for _ in 0..100 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        for action in outcome.worker_actions {
            if let ScriptWorkerAction::Start {
                id,
                url,
                document_url,
                client,
                ..
            } = action
                && url.ends_with("/child.js")
            {
                assert_eq!(document_url, "https://example.com/parent/index");
                assert_eq!(client.id, 0);
                child_id = Some(id);
            }
        }
    }
    let child_id = child_id.expect("child start was not routed");
    assert_ne!(child_id, 1);
    let result = runtime.complete_worker_event_with_loader(child_id, Ok("\"child\"".into()), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let result = runtime.complete_worker_event_with_loader(1, Ok("\"root\"".into()), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    evaluate(
        &mut runtime,
        &dom,
        "if(rootMessage!=='root' || f.contentWindow.message!=='child') throw Error('worker owner');",
    );
    let result = evaluate(&mut runtime, &dom, "f.remove();");
    assert!(
        result
            .worker_actions
            .iter()
            .any(|action| matches!(action, ScriptWorkerAction::Terminate { id } if *id==child_id))
    );
    assert!(
        !result
            .worker_actions
            .iter()
            .any(|action| matches!(action, ScriptWorkerAction::Terminate { id: 1 }))
    );
}

#[test]
fn child_cannot_forge_a_parent_worker_identifier() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.worker=new Worker('/root.js');
        const f=document.createElement('iframe');document.body.append(f);
    </script>"#,
    );
    drain(&mut runtime);
    let result = evaluate(
        &mut runtime,
        &dom,
        "f.contentWindow.eval('__hostCall(\"workerTerminate\",1);__hostCall(\"workerPostMessage\",1,\"null\")');",
    );
    assert!(
        result.worker_actions.is_empty(),
        "{:?}",
        result.worker_actions
    );
}
