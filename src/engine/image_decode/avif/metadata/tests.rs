use super::*;

#[test]
fn metadata_box_boundaries_are_fail_closed() {
    for bytes in [
        &b""[..],
        b"\0\0\0\x07meta",
        b"\0\0\0\x10meta",
        b"\0\0\0\x01meta",
    ] {
        assert!(parse(bytes).is_err());
    }
}

#[test]
fn invalid_transform_properties_are_not_ignored() {
    for (kind, bytes) in [
        (b"irot", &b"\x04"[..]),
        (b"imir", &b"\x02"[..]),
        (b"clap", &b""[..]),
    ] {
        assert!(apply_property(&mut Metadata::default(), *kind, bytes, true).is_err());
    }
}

#[test]
fn unknown_optional_properties_do_not_authorize_unknown_essential_properties() {
    assert!(apply_property(&mut Metadata::default(), *b"zzzz", b"", false).is_ok());
    assert!(apply_property(&mut Metadata::default(), *b"zzzz", b"", true).is_err());
}
