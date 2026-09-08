use super::*;
use crate::limits::MAX_DOM_TREE_MUTATIONS_PER_TASK;

#[test]
fn timer_budget_errors_identify_the_callback() {
    let (_, outcome) = execute_html(&format!(
        r#"<body><script>
            setTimeout(function youtubeBatch() {{
                for (let i = 0; i < {}; i++)
                    document.body.appendChild(document.createElement('i'));
            }}, 0);
        </script></body>"#,
        MAX_DOM_TREE_MUTATIONS_PER_TASK + 1
    ));

    assert!(
        outcome.errors.iter().any(|error| {
            error.contains("youtubeBatch")
                && error.contains("DOM tree mutation task budget exceeded")
                && error.contains(&format!("child_list={MAX_DOM_TREE_MUTATIONS_PER_TASK}"))
        }),
        "{:?}",
        outcome.errors
    );
}
