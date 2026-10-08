//! The Chrome oracle also runs through the native Window CSS and Canvas hosts.
use super::*;

#[test]
fn calculated_color_pixels_and_animation_timing_use_native_hosts() {
    let fixture = include_str!("../../../../tests/canvas/css-math-components.js");
    let (dom, outcome) = execute_html(&format!(
        "<style>@keyframes mathFade{{from{{opacity:0}}to{{opacity:1}}}}
        #animated{{animation:mathFade min(2s,1000ms) linear calc(-250ms) sqrt(4) alternate both paused}}
        </style><body><div id=animated></div><div id=results></div>
        <script>{fixture}\nrunCSSMathComponents();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let results = dom
        .elements_named("div")
        .find(|element| element.attr("id").as_deref() == Some("results"))
        .unwrap();
    assert_eq!(results.attr("data-done").as_deref(), Some("true"));
    assert_eq!(results.attr("data-count").as_deref(), Some("137"));
    assert_eq!(results.attr("data-failed").as_deref(), Some("0"));
}
