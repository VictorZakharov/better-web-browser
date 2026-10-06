use super::*;

fn values(value: &str) -> Vec<String> {
    expand(value)
        .unwrap()
        .into_iter()
        .map(|(_, value)| value)
        .collect()
}

#[test]
fn expansion_retains_authorship_and_resets_each_supported_component() {
    let actual = values("italic 600 2em / 150% 'Family, One', Arial");
    assert_eq!(
        actual,
        [
            "italic",
            "600",
            "2em",
            "150%",
            "'Family, One' , Arial",
            "normal",
            "auto",
            "normal",
            "normal"
        ]
    );
    assert_eq!(
        serialize(&actual),
        "italic 600 2em / 150% 'Family, One' , Arial"
    );
    assert_eq!(serialize(&values("16px Arial")), "16px Arial");
}

#[test]
fn wide_keywords_expand_into_every_longhand_and_serialize_only_when_uniform() {
    for keyword in ["initial", "inherit", "unset", "revert", "revert-layer"] {
        let mut actual = values(keyword);
        assert!(actual.iter().all(|value| value == keyword));
        assert_eq!(serialize(&actual), keyword);
        actual[2] = "20px".into();
        assert_eq!(serialize(&actual), "");
    }
}

#[test]
fn noninitial_reset_only_components_cannot_be_lost_when_serializing_font() {
    for (index, value) in [
        (5, "'liga' off"),
        (6, "none"),
        (7, "no-common-ligatures"),
        (8, "oldstyle-nums"),
    ] {
        let mut actual = values("20px Arial");
        actual[index] = value.into();
        assert_eq!(serialize(&actual), "");
    }
    assert_eq!(serialize(&[]), "");
    let mut actual = values("20px Arial");
    actual[4].clear();
    assert_eq!(serialize(&actual), "");
}

#[test]
fn pending_variables_are_not_resolved_or_misrepresented_as_longhand_values() {
    assert!(expand("var(--font)").is_none());
    assert!(expand("italic var(--size) Arial").is_none());
    assert_eq!(
        super::super::expand("font", "var(--font)"),
        [("font".into(), "var(--font)".into())]
    );
}
