# Deterministic Canvas test font

`ahem.ttf` is the unchanged Ahem font published by W3C, used only by tests.
It provides fixed advances and box-shaped glyphs so assertions do not depend on
which fonts are installed on the test machine. It is not a browser fallback font.

- Source: https://www.w3.org/Style/CSS/Test/Fonts/Ahem/ahem.ttf
- Authors: Todd Fahrner and Myles C. Maxfield
- Licensing: public domain; CC0 waiver where public domain is not recognized,
  as documented at https://www.w3.org/Style/CSS/Test/Fonts/Ahem/COPYING
- Retrieved: 2026-10-06
- Size: 22,572 bytes
- SHA-256: `f0a92cd0cc45735591c9b5b1fa8aecd5194e8dc518895ca22af94a46c23550dc`

The test loads these bytes through the real bounded font decoder and Fontique /
HarfRust / Swash providers. No upstream scripts or executable code are imported.
The asset is not linked into the release browser.
