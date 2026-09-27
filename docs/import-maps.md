# Document import maps

The document module loader processes an inline, parser-inserted
`<script type="importmap">` before subsequently encountered module graphs. It
normalizes URL-like specifiers against the map's base URL, matches exact entries
and longest prefixes, checks the most-specific applicable scope first, and
distinguishes an unmapped specifier from an explicit `null` block. Static
`import` declarations, document `import()`, and `import.meta.resolve()` use the
same resolver. Each document or child document owns its own map.

Multiple maps merge in encounter order. An earlier exact rule wins over a
later duplicate, but later non-conflicting rules can add more-specific keys or
scopes. A successful module resolution is recorded so a subsequent map cannot
retroactively change that resolution. Invalid JSON or a non-object map is
rejected without altering an existing map; malformed individual addresses
produce bounded diagnostics. Source size and rule count are bounded.

This follows the [HTML Standard's import-map processing and module-specifier
resolution model](https://html.spec.whatwg.org/multipage/webappapis.html#module-specifier-resolution).
The implementation does not fetch maps through a `src` attribute; HTML only
defines inline import maps. It also does not yet process script-created map
elements, implement module import attributes or non-JavaScript module types, or
apply import-map `integrity` metadata to module fetches. Until the latter is
implemented, maps containing `integrity` are rejected rather than silently
claiming subresource verification. CSP blocks unauthorized maps in both parser
paths; `document.write()` does not yet dispatch a `securitypolicyviolation`
event for that denial. The nonincremental page-script helper does not install
maps. These are browser support limits, not permission to rewrite site-specific
module paths.
