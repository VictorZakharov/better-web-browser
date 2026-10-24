//! An offline workload shaped like a long encyclopedia article: ~17k elements in a three-column
//! grid with sticky side menus, floated figures, tables, many links and a few hundred rules. The
//! page is generated (original text, fixed seed), so runs are reproducible without network
//! access or third-party content. The ignored test prints each pass's `LayoutStats` for a full
//! layout and for the small changes that dominate a settling page:
//!
//! `cargo test --release --lib layout_workload -- --ignored --nocapture`
use crate::engine::dom::{Node, NodeRef};
use crate::engine::layout::test_support::FixedMeasurer;
use crate::engine::layout::{FontSpec, TextMeasurer, layout_page_with_style_viewport};
use crate::engine::page::Page;
use std::fmt::Write;
use std::time::Instant;

const WIDTH: f32 = 1249.0;
const HEIGHT: f32 = 548.0;
const SECTIONS: usize = 480;

const WORDS: &[&str] = &[
    "course",
    "championship",
    "links",
    "golf",
    "player",
    "round",
    "open",
    "season",
    "record",
    "score",
    "club",
    "history",
    "trophy",
    "amateur",
    "professional",
    "winner",
    "field",
    "cut",
    "tournament",
    "coast",
    "prize",
    "venue",
    "final",
    "qualifying",
];

/// A small deterministic generator (64-bit LCG), so the page is the same on every run.
struct Seed(u64);

impl Seed {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % below as u64) as usize
    }

    fn word(&mut self) -> &'static str {
        WORDS[self.next(WORDS.len())]
    }
}

fn sentence(seed: &mut Seed, out: &mut String) {
    let words = 8 + seed.next(11);
    for index in 0..words {
        if index > 0 {
            out.push(' ');
        }
        let word = seed.word();
        match seed.next(25) {
            0 | 1 => write!(out, "<a href=\"#s{}\">{word}</a>", seed.next(SECTIONS)),
            2 => write!(out, "<b>{word}</b>"),
            3 => write!(out, "<span class=\"nowrap\">{word}</span>"),
            _ => write!(out, "{word}"),
        }
        .unwrap();
    }
    out.push_str(". ");
}

fn markup() -> String {
    let mut seed = Seed(7);
    let mut rules = String::new();
    for index in 0..60 {
        // Rules that match nothing on this page still cost selector matching, as most of a real
        // site's stylesheet does.
        writeln!(
            rules,
            ".skin-{index} .content p > a:not(.new){{color:#3{index:02}}}\n\
             #tool-{index}[aria-expanded=true] li{{margin:{index}px}}\n\
             .content section:is(.featured-{index}, .wide-{index}) h2{{font-size:{}px}}",
            20 + index % 5
        )
        .unwrap();
    }
    let mut sections = String::new();
    for index in 0..SECTIONS {
        write!(
            sections,
            "<section id=\"s{index}\"><h2>Section {index} {}</h2>",
            seed.word()
        )
        .unwrap();
        if index % 4 == 0 {
            sections.push_str("<figure class=\"thumb\"><div class=\"img\"></div><figcaption>");
            sentence(&mut seed, &mut sections);
            sections.push_str("</figcaption></figure>");
        }
        for _ in 0..3 {
            sections.push_str("<p>");
            for _ in 0..3 {
                sentence(&mut seed, &mut sections);
            }
            sections.push_str("</p>");
        }
        if index % 10 == 5 {
            sections.push_str("<table class=\"wikitable\"><tr>");
            for column in 0..6 {
                write!(sections, "<th>Col {column}</th>").unwrap();
            }
            sections.push_str("</tr>");
            for row in 0..10 {
                sections.push_str("<tr>");
                for column in 0..6 {
                    write!(sections, "<td>{} {}</td>", seed.word(), row * 7 + column).unwrap();
                }
                sections.push_str("</tr>");
            }
            sections.push_str("</table>");
        }
        sections.push_str("</section>");
    }
    let mut toc = String::new();
    for index in 0..SECTIONS {
        write!(
            toc,
            "<li id=\"toc-{index}\"><a href=\"#s{index}\">Section {index}</a></li>"
        )
        .unwrap();
    }
    format!(
        "<!doctype html><html><head><style>
        body{{margin:0;font:14px/1.6 sans-serif;color:#202122}}
        header{{height:50px;border-bottom:1px solid #ccc;display:flex;align-items:center;gap:16px}}
        .page{{display:grid;grid-template-columns:200px 1fr 180px;gap:24px;max-width:1400px;margin:0 auto}}
        .toc,.tools{{position:sticky;top:0;align-self:start;max-height:100vh;overflow:auto}}
        .toc ul,.tools ul{{list-style:none;margin:0;padding:0}}
        .toc li.active > a{{font-weight:bold}}
        section h2{{font:22px/1.3 serif;border-bottom:1px solid #a2a9b1;margin:20px 0 8px}}
        figure.thumb{{float:right;width:220px;margin:0 0 8px 16px;border:1px solid #c8ccd1}}
        figure .img{{height:140px;background:#eaecf0}}
        table.wikitable{{border-collapse:collapse;margin:8px 0}}
        table.wikitable td,table.wikitable th{{border:1px solid #a2a9b1;padding:2px 6px}}
        .nowrap{{white-space:nowrap}}
        .content.alt p{{color:#333}}
        {rules}
        </style></head><body>
        <header><strong>Workload</strong><span id=counter>0</span></header>
        <div class=page><nav class=toc><ul>{toc}</ul></nav>
        <main><div class=toolbar>Article Talk Read Edit</div>
        <div class=content id=content>{sections}</div></main>
        <aside class=tools id=tools><ul><li>Tools</li></ul></aside></div></body></html>"
    )
}

/// Measures text like `FixedMeasurer` and lets layout retain results across passes.
struct Retaining;

impl TextMeasurer for Retaining {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        FixedMeasurer.measure(text, font)
    }

    fn layout_cache_epoch(&self) -> Option<u64> {
        Some(0)
    }
}

fn element(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

/// Lays the page out once and prints the pass's work. Styles are brought up to date first and
/// not timed: this refresh path restyles the whole document, unlike a renderer update.
fn pass(page: &mut Page, label: &str) {
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    page.take_layout_stats();
    let started = Instant::now();
    let output = layout_page_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, &mut Retaining);
    let layout = started.elapsed();
    eprintln!(
        "{label:<10} layout {layout:>10.2?} items {:>6}\n    {:?}",
        output.items.len(),
        page.take_layout_stats()
    );
}

#[test]
#[ignore = "prints timings; run in release with --nocapture"]
fn layout_workload() {
    let mut page = Page::parse(&markup(), "https://workload.test/");
    let elements = Node::descendants(&page.dom.document)
        .filter(|node| node.element().is_some())
        .count();
    eprintln!("elements {elements}");
    for label in ["cold", "warm", "idle"] {
        pass(&mut page, label);
    }
    let entry = element(&page, "toc-7");
    for round in 0..3 {
        entry.set_attr("class", if round % 2 == 0 { "active" } else { "" });
        pass(&mut page, "toc class");
    }
    let content = element(&page, "content");
    for round in 0..2 {
        content.set_attr(
            "class",
            if round % 2 == 0 {
                "content alt"
            } else {
                "content"
            },
        );
        pass(&mut page, "recolor");
    }
    let counter = element(&page, "counter");
    for round in 1..=3 {
        Node::set_text_content(&counter, &round.to_string());
        pass(&mut page, "text");
    }
}
