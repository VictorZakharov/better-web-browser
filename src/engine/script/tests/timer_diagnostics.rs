use super::*;
use crate::limits::MAX_DOM_MUTATIONS_PER_TASK;

#[test]
fn timer_budget_errors_identify_the_callback() {
    let (_, outcome) = execute_html(&format!(
        r#"<body><script>
            setTimeout(function youtubeBatch() {{
                for (let i = 0; i < {}; i++)
                    document.body.setAttribute('data-i', String(i));
            }}, 0);
        </script></body>"#,
        MAX_DOM_MUTATIONS_PER_TASK + 1
    ));

    assert!(
        outcome.errors.iter().any(|error| {
            error.contains("youtubeBatch")
                && error.contains("DOM mutation task budget exceeded")
                && error.contains(&format!("attributes={MAX_DOM_MUTATIONS_PER_TASK}"))
                && error.contains("unchanged_attributes=0")
                && error.contains(&format!(
                    "top_attributes=[data-i:{MAX_DOM_MUTATIONS_PER_TASK}]"
                ))
                && error.contains("child_list=0")
        }),
        "{:?}",
        outcome.errors
    );
}
