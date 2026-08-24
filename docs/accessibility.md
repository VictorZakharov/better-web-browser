# Accessibility architecture

Breeze exposes an early Windows UI Automation tree through [AccessKit](https://github.com/AccessKit/accesskit). This is a technical-alpha compatibility layer, not a claim of complete assistive-technology or WAI-ARIA support.

## Ownership and update flow

The sandboxed renderer derives a semantic tree from its owned DOM, computed layout, native-control descriptions, focus, and input selection. Its first presentation carries a full tree; later presentation revisions carry changed nodes and removals. The browser validates each update transactionally, assigns browser-owned 64-bit AccessKit identities, and publishes browser chrome plus the active tab through one Windows adapter.

This follows AccessKit's full-initial-tree and incremental-update model while keeping renderer DOM identities out of the privileged platform tree. The adapter is created during window initialization, before Windows can request an object through [`WM_GETOBJECT`](https://learn.microsoft.com/windows/win32/winauto/wm-getobject), and subsequent focus/tree events are raised after UI-thread updates.

## Current semantic contract

The document tree currently projects:

- document, text, paragraph, heading, link, and button roles;
- text, search, password, multiline, and select controls;
- lists, list items, tables, rows, cells, and row/column headers;
- images with alternative text and common landmarks such as main, navigation, form, header, footer, article, and named sections;
- names from the supported `aria-label`, form metadata, text, title/description, and image-alt paths;
- document-coordinate bounds transformed by browser DPI, toolbar, and scroll state;
- focus, disabled/read-only state, heading level, editable value, and UTF-16 input selection in the internal semantic protocol.

Supported UI Automation actions are focus, invoke/click, and set value. They are routed back to the renderer only when the current validated node advertises that action. Chrome actions reuse the browser's existing navigation, tab, address, Reader, and task-manager paths.

The role projection is informed by [WAI-ARIA 1.2](https://www.w3.org/TR/wai-aria-1.2/) and the [HTML Accessibility API Mappings](https://www.w3.org/TR/html-aam-1.0/), but Breeze implements only the subset listed here.

## Trust boundary

Accessibility IPC is treated as untrusted renderer output:

- node, edge, string, geometry, and total-text budgets are checked before allocation or publication;
- full trees and deltas reject duplicate identities, stale revisions, unknown removals, invalid roots/focus, multiple parents, cycles, and unreachable nodes;
- renderer 128-bit DOM identities are remapped so they cannot collide with browser chrome;
- updates commit only after the complete candidate tree validates;
- platform actions cross a bounded queue, return to the browser UI thread, and are checked against the active cached tree before dispatch.

A malformed tree therefore crashes only the page renderer and uses the normal tab-local recovery surface.

## Known limitations

- Windows UI Automation is the only platform adapter. macOS and Linux accessibility APIs are not implemented.
- The accessible-name computation and ARIA state/relationship coverage are partial.
- Rich text ranges, hypertext navigation, live regions, document text selection, table coordinates/headers, and advanced UIA patterns are not exposed yet. Input selection is retained in the renderer protocol for future text-pattern support.
- Reader view currently exposes browser chrome but not a separate semantic document tree.
- Native page controls remain the keyboard widgets; AccessKit actions validate and route to those existing control and renderer paths.
- Automated tests cover renderer semantics, deltas, protocol limits, and browser-side validation. They do not replace hands-on testing with Narrator or another UI Automation client.

## Manual smoke test

1. Start Breeze normally and enable Windows Narrator or inspect the window with a UI Automation tool.
2. Verify that the window exposes tabs, the navigation toolbar, address field, status, and the active document in reading order.
3. Open a page with headings, links, a labeled text field, a button, a list, and a table. Confirm names, roles, focus, and non-empty bounds.
4. Invoke a link/button and edit the field through the accessibility client; verify the browser follows the existing interaction behavior and reports the new focus/value.
5. Switch tabs, scroll, toggle Reader, and crash/reload a renderer; verify stale document nodes are no longer reachable and other tabs remain usable.

The Windows provider lifecycle follows Microsoft's [UI Automation provider guidance](https://learn.microsoft.com/windows/win32/winauto/uiauto-providersoverview).

