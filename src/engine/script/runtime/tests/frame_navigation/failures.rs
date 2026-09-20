use super::*;

#[test]
fn failed_and_policy_blocked_child_navigations_release_parent_load_once() {
    for failure in ["network", "csp", "xfo", "mime"] {
        let (dom, mut runtime) = start(
            r#"<body><script>
            window.loads=[]; addEventListener('load',()=>loads.push('parent'));
            const f=document.createElement('iframe'); f.src='/child';
            f.onload=()=>loads.push('frame'); f.onerror=()=>loads.push('ERROR');
            document.body.append(f);
        </script>"#,
        );
        let id = runtime
            .advance_time(Duration::ZERO, 1)
            .fetch_actions
            .into_iter()
            .find_map(|a| {
                if let ScriptFetchAction::Start { id, .. } = a {
                    Some(id)
                } else {
                    None
                }
            })
            .unwrap();
        runtime.finish_document_lifecycle();
        drain(&mut runtime);
        evaluate(
            &mut runtime,
            &dom,
            "if(loads.length) throw Error('load before navigation finished');",
        );
        let result = if failure == "network" {
            Err(crate::fetch::FetchError::new(
                crate::fetch::FetchErrorKind::Network,
                "fixture failure",
            ))
        } else {
            let mut response = response(
                "https://example.com/child",
                "<script>parent.loads.push('BAD')</script>",
            );
            let header = match failure {
                "csp" => ("Content-Security-Policy", "frame-ancestors 'none'"),
                "xfo" => ("X-Frame-Options", "DENY"),
                _ => ("Content-Type", "application/octet-stream"),
            };
            response.headers.append(header.0, header.1).unwrap();
            Ok(response)
        };
        runtime.complete_fetch_with_loader(id, result, None);
        drain(&mut runtime);
        evaluate(
            &mut runtime,
            &dom,
            "if(loads.join(',')!=='frame,parent') throw Error(loads);",
        );
        assert!(runtime.document_load_finished(), "{failure}");
    }
}
