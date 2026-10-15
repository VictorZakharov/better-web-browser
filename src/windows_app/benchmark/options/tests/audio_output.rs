use super::*;

#[test]
fn audio_output_probe_requires_hidden_benchmark_and_is_opt_in() {
    let parse = |arguments: Vec<&str>| {
        LaunchOptions::parse_from(Instant::now(), arguments.into_iter().map(str::to_string))
    };
    assert!(parse(vec!["--audio-output-probe"]).is_err());
    let normal = parse(vec![
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
    ])
    .unwrap();
    assert!(!normal.benchmark.unwrap().audio_output_probe);
    let probe = parse(vec![
        "--benchmark",
        "about:blank",
        "--output",
        "result.json",
        "--audio-output-probe",
    ])
    .unwrap();
    assert!(probe.benchmark.unwrap().audio_output_probe);
}
