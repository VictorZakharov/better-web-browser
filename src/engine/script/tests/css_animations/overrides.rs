use super::assert_script;

#[test]
fn successful_api_changes_own_only_corresponding_css_animation_inputs() {
    let fixture = include_str!("../../../../../tests/canvas/css-animation-overrides.js");
    assert_script("", &format!(
        "const results=document.createElement('div');results.id='results';document.body.appendChild(results);
        {fixture}
        const oracleFailures=runCSSAnimationOverrides();
        check('native ownership oracle: '+oracleFailures.join(','),oracleFailures.length===0);
        check('ownership oracle count: '+results.getAttribute('data-count'),results.getAttribute('data-count')==='83');"
    ));
}
