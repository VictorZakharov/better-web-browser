# Inert template ownership and activation

HTML template contents belong to a separate document with no browsing context or
default custom-element registry. Breeze shares one such document per owner and
reuses it for nested templates, preserving whether the owner is HTML or XML.
Moving a template to another document updates its contents to the destination's
inert owner. Cloning/importing a template preserves this boundary; importing a
template's *contents* into the live document instead makes those elements eligible
for its registry.

This implements the [HTML template owner-document algorithm](https://html.spec.whatwg.org/multipage/scripting.html#appropriate-template-contents-owner-document).
The native host preallocates the inert document alongside each document, with
both counted against the existing DOM-node budget. This is unobservable before
the first template is created and avoids allocations halfway through adoption.

The [DOM insertion algorithm](https://dom.spec.whatwg.org/#concept-node-insert)
only attempts custom-element upgrades for connected descendants. Moving inert
content into a detached cache therefore does not invoke constructors, even if
adoption changes its owner to the live document. Connecting that content or
explicitly importing it into the live document performs the appropriate upgrade.
Already upgraded elements retain their lifecycle callbacks across adoption.
This does not implement the full scoped custom-element registry specification.

## Regression evidence

YouTube's reusable templates exposed this defect: premature custom-element
constructors inserted comments into cached template trees, invalidating previously
recorded child-index paths. Later stamping failed when assigning `__dataHost` to
a node that no longer existed at the expected path. The fix applies to document
ownership and activation, without production code recognizing YouTube or Polymer.

Generic regressions cover shared/nested ownership, adoption, HTML/XML case rules,
clone/import behavior, constructor-free template caching, and upgrade on connection.
