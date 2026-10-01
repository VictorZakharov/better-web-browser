use super::*;

// Each input channel carries its own impulse. Checking the whole output block
// catches cross-talk, omitted channels, and unwanted destination-dependent
// processing, rather than verifying a single conveniently symmetric sample.
fn check_matrix(source: usize, destination: usize, matrix: &str) {
    let html = format!(
        r#"<body><script>
        const sourceChannels = {source}, outputChannels = {destination};
        const matrix = {matrix};
        const context = new OfflineAudioContext(outputChannels, 128, 8000);
        context.destination.channelInterpretation = 'discrete';
        const input = context.createBuffer(sourceChannels, 128, 8000);
        for (let channel = 0; channel < sourceChannels; ++channel)
            input.getChannelData(channel)[channel * 2] = 1;
        const source = new AudioBufferSourceNode(context, {{buffer: input}});
        const mix = new GainNode(context, {{channelCount: outputChannels,
            channelCountMode: 'explicit', channelInterpretation: 'speakers'}});
        source.connect(mix).connect(context.destination);
        source.start();
        context.startRendering().then(result => {{
            for (let channel = 0; channel < outputChannels; ++channel) {{
                const pcm = result.getChannelData(channel);
                for (let frame = 0; frame < pcm.length; ++frame) {{
                    const expected = frame % 2 === 0 && frame < sourceChannels * 2 ?
                        matrix[channel][frame / 2] : 0;
                    if (Math.abs(pcm[frame] - expected) > 0.000001)
                        throw Error('matrix {source}->{destination} channel ' + channel +
                            ' frame ' + frame + ': ' + pcm[frame] + ' != ' + expected);
                }}
            }}
            console.log('speaker matrix passed');
        }});
    </script>"#
    );
    let (_, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: speaker matrix passed"]);
}

#[test]
fn mono_to_mono_is_identity() {
    check_matrix(1, 1, "[[1]]");
}

#[test]
fn mono_to_stereo_duplicates_left_and_right() {
    check_matrix(1, 2, "[[1],[1]]");
}

#[test]
fn mono_to_quad_leaves_surround_silent() {
    check_matrix(1, 4, "[[1],[1],[0],[0]]");
}

#[test]
fn mono_to_surround_routes_only_to_center() {
    check_matrix(1, 6, "[[0],[0],[1],[0],[0],[0]]");
}

#[test]
fn stereo_to_mono_averages_left_and_right() {
    check_matrix(2, 1, "[[0.5,0.5]]");
}

#[test]
fn stereo_to_stereo_preserves_separation() {
    check_matrix(2, 2, "[[1,0],[0,1]]");
}

#[test]
fn stereo_to_quad_leaves_surround_silent() {
    check_matrix(2, 4, "[[1,0],[0,1],[0,0],[0,0]]");
}

#[test]
fn stereo_to_surround_leaves_center_lfe_and_surround_silent() {
    check_matrix(2, 6, "[[1,0],[0,1],[0,0],[0,0],[0,0],[0,0]]");
}

#[test]
fn quad_to_mono_averages_all_four_speakers() {
    check_matrix(4, 1, "[[0.25,0.25,0.25,0.25]]");
}

#[test]
fn quad_to_stereo_averages_front_and_back_on_each_side() {
    check_matrix(4, 2, "[[0.5,0,0.5,0],[0,0.5,0,0.5]]");
}

#[test]
fn quad_to_quad_preserves_each_speaker() {
    check_matrix(4, 4, "[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]]");
}

#[test]
fn quad_to_surround_inserts_silent_center_and_lfe() {
    check_matrix(
        4,
        6,
        "[[1,0,0,0],[0,1,0,0],[0,0,0,0],[0,0,0,0],[0,0,1,0],[0,0,0,1]]",
    );
}

#[test]
fn surround_to_mono_omits_lfe_and_uses_normative_center_gain() {
    check_matrix(6, 1, "[[Math.SQRT1_2,Math.SQRT1_2,1,0,0.5,0.5]]");
}

#[test]
fn surround_to_stereo_omits_lfe() {
    check_matrix(
        6,
        2,
        "[[1,0,Math.SQRT1_2,0,Math.SQRT1_2,0],[0,1,Math.SQRT1_2,0,0,Math.SQRT1_2]]",
    );
}

#[test]
fn surround_to_quad_preserves_surround_and_omits_lfe() {
    check_matrix(
        6,
        4,
        "[[1,0,Math.SQRT1_2,0,0,0],[0,1,Math.SQRT1_2,0,0,0],[0,0,0,0,1,0],[0,0,0,0,0,1]]",
    );
}

#[test]
fn surround_to_surround_preserves_lfe_as_well_as_speakers() {
    check_matrix(
        6,
        6,
        "[[1,0,0,0,0,0],[0,1,0,0,0,0],[0,0,1,0,0,0],[0,0,0,1,0,0],[0,0,0,0,1,0],[0,0,0,0,0,1]]",
    );
}

#[test]
fn speaker_interpretation_of_unknown_input_layout_is_discrete() {
    check_matrix(3, 2, "[[1,0,0],[0,1,0]]");
    check_matrix(3, 1, "[[1,0,0]]");
    check_matrix(5, 2, "[[1,0,0,0,0],[0,1,0,0,0]]");
}

#[test]
fn speaker_interpretation_of_unknown_output_layout_is_discrete() {
    check_matrix(2, 3, "[[1,0],[0,1],[0,0]]");
    check_matrix(1, 3, "[[1],[0],[0]]");
    check_matrix(4, 3, "[[1,0,0,0],[0,1,0,0],[0,0,1,0]]");
}
