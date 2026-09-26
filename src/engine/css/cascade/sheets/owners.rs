//! CSSOM's document/shadow-root style-sheet list follows owner tree order,
//! independently of response completion. Disconnected or disabled owners contribute no rules.
//! https://drafts.csswg.org/cssom/#document-css-style-sheets
use super::*;
#[cfg(test)]
mod tests;

pub(super) fn append(
    root: &NodeRef,
    base_url: &str,
    resources: &[StylesheetSource],
    environment: MediaEnvironment,
    inputs: &mut Vec<SheetInput>,
    scope: RuleScope,
) {
    let loaded: HashMap<_, _> = resources
        .iter()
        .filter_map(|source| source.owner_url.as_deref().map(|url| (url, source)))
        .collect();
    for node in Node::descendants(root) {
        if !matches!(node.tag_name(), Some("style" | "link")) || Node::sheet_disabled(&node) {
            continue;
        }
        if node
            .attr("type")
            .is_some_and(|kind| !kind.is_empty() && !kind.eq_ignore_ascii_case("text/css"))
        {
            continue;
        }
        let overrides = node.sheet_overrides();
        let own = overrides.iter().find(|s| s.path.is_empty());
        if own
            .map(|s| s.media.clone())
            .or_else(|| node.attr("media"))
            .is_some_and(|query| {
                !query.trim().is_empty()
                    && !media::media_matches_for_environment(&query, environment)
            })
        {
            continue;
        }
        let (mut source, sheet_base, mut imports) = if node.tag_name() == Some("style") {
            let source = node.text_content();
            let imports = crate::engine::css::imports::parse(&source);
            (source, base_url.to_string(), imports)
        } else {
            if !node
                .attr("rel")
                .unwrap_or_default()
                .split_ascii_whitespace()
                .any(|rel| rel.eq_ignore_ascii_case("stylesheet"))
            {
                continue;
            }
            let Some(url) = node
                .attr("href")
                .filter(|href| !href.trim().is_empty())
                .and_then(|href| crate::engine::css::imports::resolve(base_url, &href))
            else {
                continue;
            };
            let Some(resource) = loaded.get(url.as_str()) else {
                continue;
            };
            (
                resource.source.clone(),
                resource.base_url.clone(),
                resource.imports.clone(),
            )
        };
        if let Some(own) = own {
            source.clone_from(&own.source);
            imports = crate::engine::css::imports::parse(&source);
        }
        let owner = inputs.len() as u32;
        let implicit_scope_root = node
            .parent()
            .filter(|parent| parent.element().is_some())
            .map(|parent| parent.id())
            .or_else(|| default_scope_root(root));
        let expansion = crate::engine::css::imports::expand_owned(
            &sheet_base,
            &imports,
            resources,
            environment,
            &overrides,
        );
        let mut declarations = expansion.layer_declarations.into_iter().peekable();
        for (index, imported) in expansion.sheets.into_iter().enumerate() {
            while declarations.peek().is_some_and(|(at, _)| *at == index) {
                let (_, layer) = declarations.next().unwrap();
                inputs.push(SheetInput::layer_marker(
                    import_owner_path(&layer, owner),
                    scope,
                ));
            }
            inputs.push(SheetInput {
                source: imported.source.clone(),
                base_url: imported.base_url.clone(),
                scope,
                implicit_scope_root,
                layer_prefix: import_owner_path(&imported.layer_prefix, owner),
                import_scope_prefixes: imported.scope_prefixes,
                declared_layer: None,
            });
        }
        for (_, layer) in declarations {
            inputs.push(SheetInput::layer_marker(
                import_owner_path(&layer, owner),
                scope,
            ));
        }
        inputs.push(SheetInput {
            source,
            base_url: sheet_base,
            scope,
            implicit_scope_root,
            layer_prefix: Vec::new(),
            import_scope_prefixes: Vec::new(),
            declared_layer: None,
        });
    }
}
