//! Browser-focused bidi, script, fallback, and OpenType shaping.

use super::catalog::{FontCatalog, SelectedFont};
use crate::engine::FontSpec;
use harfrust::{Direction, ShaperData, ShaperInstance, UnicodeBuffer, Variation};
use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};
use unicode_bidi::BidiInfo;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

pub(super) struct ShapedGlyph {
    pub(super) font: SelectedFont,
    pub(super) glyph_id: u16,
    pub(super) x: f32,
    pub(super) baseline: f32,
}

pub(super) struct ShapeOutput {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) glyphs: Vec<ShapedGlyph>,
    pub(super) font_select_time: Duration,
    pub(super) open_type_time: Duration,
}

pub(super) struct TextShaper {
    data: HashMap<(u64, u32), ShaperData>,
    buffer: Option<UnicodeBuffer>,
}

impl Default for TextShaper {
    fn default() -> Self {
        Self {
            data: HashMap::new(),
            buffer: Some(UnicodeBuffer::new()),
        }
    }
}

impl TextShaper {
    pub(super) fn clear(&mut self) {
        self.data.clear();
        self.buffer = Some(UnicodeBuffer::new());
    }

    pub(super) fn shape(
        &mut self,
        catalog: &mut FontCatalog,
        text: &str,
        spec: &FontSpec,
    ) -> ShapeOutput {
        let size = spec.size.clamp(1.0, 768.0);
        let height = (size * 1.2).max(size);
        let mut output = ShapeOutput {
            width: 0.0,
            height,
            glyphs: Vec::new(),
            font_select_time: Duration::ZERO,
            open_type_time: Duration::ZERO,
        };
        if text.is_empty() {
            return output;
        }

        let bidi = BidiInfo::new(text, None);
        for paragraph in &bidi.paragraphs {
            let (_, visual_runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
            for visual_run in visual_runs {
                let rtl = bidi.levels[visual_run.start].is_rtl();
                let select_started = Instant::now();
                let mut font_runs = select_font_runs(catalog, text, visual_run, rtl, spec);
                output.font_select_time += select_started.elapsed();
                if rtl {
                    font_runs.reverse();
                }
                for run in font_runs {
                    let shape_started = Instant::now();
                    self.shape_font_run(text, spec, height, run, &mut output);
                    output.open_type_time += shape_started.elapsed();
                }
            }
        }
        output
    }

    fn shape_font_run(
        &mut self,
        text: &str,
        spec: &FontSpec,
        line_height: f32,
        run: FontRun,
        output: &mut ShapeOutput,
    ) {
        let run_text = &text[run.range];
        let Ok(font_ref) =
            harfrust::FontRef::from_index(run.font.font.blob.as_ref(), run.font.font.index)
        else {
            return;
        };
        let data = self
            .data
            .entry((run.font.font.blob.id(), run.font.font.index))
            .or_insert_with(|| ShaperData::new(&font_ref));
        let variations = run
            .font
            .font
            .synthesis
            .variation_settings()
            .iter()
            .map(|(tag, value)| Variation {
                tag: harfrust::Tag::new(&tag.to_be_bytes()),
                value: *value,
            })
            .collect::<Vec<_>>();
        let instance = ShaperInstance::from_variations(&font_ref, variations);
        let shaper = data
            .shaper(&font_ref)
            .instance(Some(&instance))
            .point_size(Some(spec.size))
            .build();
        let mut buffer = self.buffer.take().unwrap_or_default();
        buffer.push_str(run_text);
        buffer.set_direction(if run.rtl {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        });
        if let Some(script) = harfrust::Script::from_iso15924_tag(harfrust::Tag::new(
            &run.script.as_iso15924_tag().to_be_bytes(),
        )) {
            buffer.set_script(script);
        }
        buffer.guess_segment_properties();
        let glyph_buffer = shaper.shape(buffer, &[]);
        let scale = spec.size.clamp(1.0, 768.0) / shaper.units_per_em().max(1) as f32;
        let baseline = font_baseline(&run.font, spec.size, line_height);
        let infos = glyph_buffer.glyph_infos();
        let positions = glyph_buffer.glyph_positions();
        let mut cursor = output.width;
        for (index, (info, position)) in infos.iter().zip(positions).enumerate() {
            let Ok(glyph_id) = u16::try_from(info.glyph_id) else {
                continue;
            };
            output.glyphs.push(ShapedGlyph {
                font: run.font.clone(),
                glyph_id,
                x: cursor + position.x_offset as f32 * scale,
                baseline: baseline - position.y_offset as f32 * scale,
            });
            cursor += position.x_advance as f32 * scale;
            if infos
                .get(index + 1)
                .is_none_or(|next| next.cluster != info.cluster)
            {
                cursor += spec.letter_spacing;
                if run_text
                    .get(info.cluster as usize..)
                    .and_then(|tail| tail.chars().next())
                    .is_some_and(char::is_whitespace)
                {
                    cursor += spec.word_spacing;
                }
            }
        }
        output.width = cursor.max(output.width);
        self.buffer = Some(glyph_buffer.clear());
    }
}

struct FontRun {
    range: Range<usize>,
    script: Script,
    rtl: bool,
    font: SelectedFont,
}

fn select_font_runs(
    catalog: &mut FontCatalog,
    text: &str,
    range: Range<usize>,
    rtl: bool,
    spec: &FontSpec,
) -> Vec<FontRun> {
    let slice = &text[range.clone()];
    let mut clusters = slice
        .grapheme_indices(true)
        .map(|(start, cluster)| Cluster {
            range: (range.start + start)..(range.start + start + cluster.len()),
            script: cluster_script(cluster),
        })
        .collect::<Vec<_>>();
    resolve_common_scripts(&mut clusters);

    let mut runs: Vec<FontRun> = Vec::new();
    for cluster in clusters {
        let cluster_text = &text[cluster.range.clone()];
        let Some(font) = catalog.select(&spec.family, spec, cluster.script, cluster_text) else {
            continue;
        };
        if let Some(last) = runs.last_mut()
            && last.script == cluster.script
            && last.font.instance == font.instance
            && last.range.end == cluster.range.start
        {
            last.range.end = cluster.range.end;
        } else {
            runs.push(FontRun {
                range: cluster.range,
                script: cluster.script,
                rtl,
                font,
            });
        }
    }
    runs
}

struct Cluster {
    range: Range<usize>,
    script: Script,
}

fn cluster_script(cluster: &str) -> Script {
    cluster
        .chars()
        .map(|ch| ch.script())
        .find(|script| is_specific_script(*script))
        .unwrap_or(Script::Common)
}

fn resolve_common_scripts(clusters: &mut [Cluster]) {
    for index in 0..clusters.len() {
        if is_specific_script(clusters[index].script) {
            continue;
        }
        clusters[index].script = clusters[..index]
            .iter()
            .rev()
            .find_map(|cluster| is_specific_script(cluster.script).then_some(cluster.script))
            .or_else(|| {
                clusters[index + 1..].iter().find_map(|cluster| {
                    is_specific_script(cluster.script).then_some(cluster.script)
                })
            })
            .unwrap_or(Script::Latin);
    }
}

fn is_specific_script(script: Script) -> bool {
    !matches!(script, Script::Common | Script::Inherited | Script::Unknown)
}

fn font_baseline(font: &SelectedFont, size: f32, line_height: f32) -> f32 {
    let Some(swash_font) =
        swash::FontRef::from_index(font.font.blob.as_ref(), font.font.index as usize)
    else {
        return line_height * 0.8;
    };
    let metrics = swash_font.metrics(&[]);
    let ascent = metrics.ascent / metrics.units_per_em.max(1) as f32 * size;
    (line_height - size).max(0.0) * 0.5 + ascent
}
