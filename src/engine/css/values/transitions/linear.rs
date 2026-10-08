//! Validate CSS Easing Level 2 linear() stops using the existing CSS tokenizer.
//! Missing/descending percentages are valid and normalized by the shared sampler.
//! https://drafts.csswg.org/css-easing-2/#linear-easing-function
pub(super) fn valid(value: &str) -> bool {
    value.starts_with("linear(") && super::normalize_easing(value).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_distributed_doubled_descending_and_extrapolated_stops() {
        for source in [
            "linear(0,1)",
            "linear(0, .5, 1)",
            "linear(0 0% 20%, 1 80% 100%)",
            "linear(0 -10%, 1 110%)",
            "linear(0 75%, .5 25%, 1)",
            "linear(-1, 2)",
        ] {
            assert!(valid(source), "{source}");
        }
    }
    #[test]
    fn rejects_missing_outputs_extra_components_and_unbounded_stop_lists() {
        for source in [
            "linear()",
            "linear(0)",
            "linear(0,)",
            "linear(0, 10%)",
            "linear(0 1, 1)",
            "linear(0 0% 20% 40%,1)",
            "linear(0px,1)",
            "linear(nan,1)",
        ] {
            assert!(!valid(source), "{source}");
        }
        assert!(!valid(
            &("linear(".to_owned()
                + &std::iter::repeat_n("0", 65).collect::<Vec<_>>().join(",")
                + ")")
        ));
    }
}
