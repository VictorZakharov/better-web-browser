//! Conditional groups and layer identities reuse the stylesheet's native syntax helpers.
use super::*;

pub(in crate::engine::css) struct Collection {
    pub(in crate::engine::css) definitions: Vec<KeyframeDefinition>,
    pub(in crate::engine::css) layers: Vec<LayerPath>,
}

pub(in crate::engine::css) fn collect_with_layers(
    css: &str,
    environment: MediaEnvironment,
    scope: RuleScope,
) -> Collection {
    let (css, _) = bounded_utf8_prefix(css, MAX_CSS_SOURCE_BYTES);
    let source = strip_comments(css);
    let mut collector = Collector {
        environment,
        scope,
        next_anonymous: 1,
        result: Collection {
            definitions: Vec::new(),
            layers: Vec::new(),
        },
    };
    collector.rules(&source, 0, &[], false);
    collector.result
}

struct Collector {
    environment: MediaEnvironment,
    scope: RuleScope,
    next_anonymous: u64,
    result: Collection,
}

impl Collector {
    fn declare(&mut self, path: LayerPath) {
        if self.result.layers.len() < MAX_CSS_RULES_PER_STYLESHEET {
            self.result.layers.push(path);
        }
    }

    fn rules(&mut self, css: &str, depth: usize, layer: &[LayerSegment], in_style: bool) {
        if depth >= MAX_CSS_NESTING_DEPTH {
            return;
        }
        for item in nesting::block_items(css) {
            if self.result.definitions.len() == MAX_KEYFRAME_DEFINITIONS {
                break;
            }
            match item {
                nesting::BlockItem::Statement(prelude) => {
                    if let Some(paths) = parse_layer_statement(prelude) {
                        for local in paths {
                            let mut path = layer.to_vec();
                            path.extend(local);
                            self.declare(path);
                        }
                    }
                }
                nesting::BlockItem::Declarations(_) => {}
                nesting::BlockItem::Rule { prelude, body, .. } => {
                    if let Some(prelude) = at_rule_prelude(prelude, "keyframes")
                        .or_else(|| at_rule_prelude(prelude, "-webkit-keyframes"))
                    {
                        if !in_style && let Some(name) = name(prelude) {
                            self.result.definitions.push(KeyframeDefinition {
                                name,
                                blocks: blocks(body),
                                scope: self.scope,
                                layer: layer.to_vec(),
                                layer_rank: u32::MAX,
                            });
                        }
                    } else if let Some(name) = layer_prelude(prelude) {
                        let mut path = layer.to_vec();
                        if name.is_empty() {
                            path.push(LayerSegment::Anonymous(self.next_anonymous));
                            self.next_anonymous = self.next_anonymous.saturating_add(1);
                        } else if let Some(local) = parse_layer_name(name) {
                            path.extend(local);
                        } else {
                            continue;
                        }
                        self.declare(path.clone());
                        self.rules(body, depth + 1, &path, in_style);
                    } else {
                        let enabled = if at_rule_prelude(prelude, "media").is_some() {
                            media::media_matches_for_environment(prelude, self.environment)
                        } else if at_rule_prelude(prelude, "supports").is_some() {
                            supports::supports_matches(prelude)
                        } else if let Some(boundaries) = at_rule_prelude(prelude, "scope") {
                            scope::parse_scope_prelude(boundaries, depth > 0, None).is_some()
                        } else {
                            !prelude.starts_with('@')
                        };
                        if enabled {
                            self.rules(
                                body,
                                depth + 1,
                                layer,
                                in_style || !prelude.starts_with('@'),
                            );
                        }
                    }
                }
            }
        }
    }
}
