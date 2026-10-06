use super::*;

fn families(css: &str) -> Vec<String> {
    collect(
        css,
        "https://example.test/css/main.css",
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    )
    .into_iter()
    .map(|face| face.family)
    .collect()
}

#[test]
fn active_conditional_groups_preserve_face_order() {
    assert_eq!(
        families(
            r#"
      @font-face {font-family:First;src:url(a.woff)}
      @media (min-width:700px) {
        @supports (display:block) {
          @layer fonts {@font-face{font-family:Second;src:url(b.woff)}}
        }
      }
      @media (min-width:900px) {@font-face{font-family:Wrong;src:url(c.woff)}}
      @supports (made-up: no) {@font-face{font-family:Wrong;src:url(d.woff)}}
      @font-face {font-family:Last;src:url(e.woff)}
    "#
        ),
        ["First", "Second", "Last"]
    );
}

#[test]
fn strings_comments_unknown_groups_and_qualified_rules_do_not_define_faces() {
    assert_eq!(
        families(
            r#"
      /* @font-face {font-family:Comment;src:url(a.woff)} */
      p {content:"@font-face{font-family:String;src:url(b.woff)}";
         @font-face {font-family:Nested;src:url(c.woff)}}
      @unknown {@font-face {font-family:Unknown;src:url(d.woff)}}
      @font-face-extra {font-family:Wrong;src:url(e.woff)}
      @font-face extra {font-family:Wrong;src:url(f.woff)}
      @font-face {font-family:Actual;src:url(g.woff)}
    "#
        ),
        ["Actual"]
    );
}

#[test]
fn group_depth_and_face_count_are_bounded() {
    let deep = format!(
        "{}@font-face{{font-family:Deep;src:url(a.woff)}}{}",
        "@media all{".repeat(MAX_CSS_NESTING_DEPTH + 1),
        "}".repeat(MAX_CSS_NESTING_DEPTH + 1)
    );
    assert!(families(&deep).is_empty());
    let css = (0..100)
        .map(|index| format!("@font-face{{font-family:F{index};src:url(f{index}.woff)}}"))
        .collect::<String>();
    assert_eq!(families(&css).len(), MAX_CONNECTED_FACES);
}

#[test]
fn bases_are_kept_for_active_urls_and_print_sheets_are_excluded() {
    let css = "@media print{@font-face{font-family:Print;src:url(p.woff)}}\
      @media screen{@font-face{font-family:Screen;src:url(../screen.woff)}}";
    let faces = collect(
        css,
        "https://example.test/css/main.css",
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].url, "https://example.test/screen.woff");
}
