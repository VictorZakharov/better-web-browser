# DOM traversal, editing state, and drag data

PR #180 extends the browser's DOM compatibility layer alongside Canvas. The
implementations use the renderer-owned node tree and existing mutation path;
they do not keep a second, independently editable DOM. This matters for pages
that traverse, mutate, or select nodes while rendering is in progress.

## Traversal and tree position

- `Document.createNodeIterator()` exposes root, mask, filter, reference node,
  and before/after pointer. It walks in preorder and visits descendants of a
  rejected node; unlike TreeWalker, `FILTER_REJECT` does not prune a subtree.
  The root itself is eligible. `detach()` remains a compatibility no-op.
- Both NodeIterator and TreeWalker reject recursive filter invocation. A
  NodeIterator filter exception leaves its committed reference unchanged.
  A filter callback may remove a candidate; the pre-removal algorithm repairs
  the pending pointer before traversal resumes. Removal, insertion/movement,
  `innerHTML`, `textContent`, and adoption use the same adjustment path.
- `Node.compareDocumentPosition()` returns ancestry and sibling-order masks,
  with attribute nodes compared adjacent to their owner element. Disconnected
  nodes receive a stable implementation-specific direction, not a false
  connected-tree order. The six mask constants are exposed on Node and its
  prototype.
- `lookupPrefix()`, `lookupNamespaceURI()`, and `isDefaultNamespace()` resolve
  namespace declarations through the parent-element chain and honor local
  shadowing by an empty default declaration. `Node.normalize()` merges
  adjacent exclusive Text nodes, removes empty Text nodes, and adjusts live
  Range boundary points into the retained node.

## Selection and editing state

- Range clone/extract/delete operations distinguish contained siblings from
  partially contained edge descendants. `insertNode()` and
  `surroundContents()` validate hierarchy before editing. Live ranges track
  child-list and CharacterData changes; `StaticRange` keeps its fixed boundary
  values. `Selection.extend()` preserves the anchor and changes direction when
  the focus crosses it.
- `contentEditable`, `isContentEditable`, and `Document.designMode` reflect
  HTML's enumerated/inherited editable state. CSS `:read-write` and
  `:read-only` use that state rather than treating every element as editable.
  This is not yet a complete rich-text editor: keyboard editing commands,
  clipboard integration, and selection painting remain separate work.

## Drag data

- `DataTransfer`, `DataTransferItemList`, `DataTransferItem`, and `FileList`
  expose string and File payloads with normalized MIME types. A pointer drag
  shares one store through trusted drag events. It is writable during
  `dragstart`, protected during motion, readable during `drop`, and disabled
  afterward. A target must cancel `dragover` and negotiate an allowed effect
  before the drop can deliver data.
- The current interaction is same-document pointer drag. Drag previews,
  cross-window drag, native OS files, and clipboard-backed DataTransfer are
  not provided. Exposing the interfaces is not a claim that those workflows
  are implemented.

The behavior follows the [DOM Standard](https://dom.spec.whatwg.org/),
[HTML editing](https://html.spec.whatwg.org/multipage/interaction.html#editing),
and [HTML drag and drop](https://html.spec.whatwg.org/multipage/dnd.html).
Tests cover traversal order and removal during a filter callback, namespace
inheritance, live-range normalization, selection direction, and drag-store
phase security.
