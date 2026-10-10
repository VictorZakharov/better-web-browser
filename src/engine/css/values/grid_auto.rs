//! Implicit-grid properties use the same bounded track grammar as explicit grids.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct GridAutoFlow {
    pub column: bool,
    pub dense: bool,
}

impl GridAutoFlow {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut flow = Self::default();
        let mut axis = false;
        let parts = borrowed_components(value)?;
        if parts.is_empty() {
            return None;
        }
        for part in parts {
            match ident(part)?.as_str() {
                "row" | "column" if !axis => {
                    flow.column = ident(part)?.as_str() == "column";
                    axis = true;
                }
                "dense" if !flow.dense => flow.dense = true,
                _ => return None,
            }
        }
        Some(flow)
    }
    pub(crate) fn css_text(self) -> &'static str {
        match (self.column, self.dense) {
            (false, false) => "row",
            (true, false) => "column",
            (false, true) => "row dense",
            (true, true) => "column dense",
        }
    }
}

pub(crate) fn tracks_valid(value: &str) -> bool {
    borrowed_components(value).is_some_and(|parts| {
        !parts.is_empty()
            && parts.iter().all(|part| {
                !part.contains('[')
                    && function(part).is_none_or(|(name, _)| name != "repeat")
                    && grid_tracks::parse(part).is_some_and(|tracks| tracks.len() == 1)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flow_keywords_and_implicit_track_admission_are_atomic() {
        for value in ["row", "column", "dense", "dense column", "row dense"] {
            assert!(GridAutoFlow::parse(value).is_some(), "{value}");
        }
        for value in [
            "",
            "row column",
            "dense dense",
            "column row dense",
            "row nonsense",
        ] {
            assert!(GridAutoFlow::parse(value).is_none(), "{value}");
        }
        for value in ["auto", "40px 60px", "calc(25% - 7.5px)", "minmax(0,1fr)"] {
            assert!(tracks_valid(value), "{value}");
        }
        for value in [
            "",
            "none",
            "repeat(2,40px)",
            "[start] 20px",
            "minmax(1fr,20px)",
            "-10px",
        ] {
            assert!(!tracks_valid(value), "{value}");
        }
    }
}
