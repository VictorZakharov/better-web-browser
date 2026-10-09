//! Preserve the standalone full-page parser's eager image metadata contract.
//! The live incremental loader uses from_dom and rasterizes after the cascade.
use super::*;

impl Page {
    pub(super) fn prepare_standalone_svgs(&mut self) {
        for svg in Node::shadow_including_descendants(&self.dom.document)
            .filter(|node| node.tag_name() == Some("svg"))
            .take(MAX_INLINE_SVGS)
        {
            let input = svg::InlineSvgInput::new(&svg, None);
            self.inline_svg_versions.insert(svg.id(), input.version);
            if let Ok(image) = input.decode() {
                let _ = media::install_initial_decoded_image(
                    &mut self.images,
                    inline_svg_key(&svg),
                    image,
                );
            }
        }
    }
}
