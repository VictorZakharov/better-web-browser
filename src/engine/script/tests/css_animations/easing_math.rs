use super::assert_script;

#[test]
fn mathematical_easing_drives_css_and_web_animations_samples() {
    let fixture = include_str!("../../../../../tests/canvas/css-math-easing.js");
    assert_script("@keyframes mathEase{from{opacity:0}to{opacity:1}}", &format!(
        "const results=document.createElement('div');results.id='results';document.body.appendChild(results);
        {fixture}
        const oracleFailures=runCSSMathEasing();
        check('native easing oracle: '+oracleFailures.join(','), oracleFailures.length === 0);
        check('complete oracle count', results.getAttribute('data-count') === '245');"
    ));
}
