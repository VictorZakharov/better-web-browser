use super::*;

#[test]
fn native_paint_trace_is_explicit_and_cannot_enable_interactive_automation() {
    let parse = |arguments: Vec<&str>| {
        LaunchOptions::parse_from(Instant::now(), arguments.into_iter().map(str::to_string))
    };
    assert!(parse(vec!["--native-paint-trace"]).is_err());
    let ordinary = parse(vec![
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
    ])
    .unwrap();
    assert!(ordinary.benchmark.unwrap().native_paint.is_none());
    let trace = parse(vec![
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
        "--native-paint-trace",
    ])
    .unwrap();
    assert!(trace.benchmark.unwrap().native_paint.is_some());
}
