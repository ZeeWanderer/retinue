# Overlay Assets

Lato uses Regular, Semibold and Bold under the adjacent SIL Open Font License.
Regular/Bold come from Google Fonts (`ofl/lato`). Semibold is Lato 2.010,
decoded from the reference's WOFF to TrueType under the same license; it matches
the table's 600 weight and font version.

Item text uses Fontin SmallCaps by Jos Buivenga. The build fetches and verifies
the author's archive into `.cache/fonts/fontin`, then embeds the font. The
standalone font is not checked in or installed. See
[Fontin](https://www.exljbris.com/fontin.html) and the
[free-font license](https://www.exljbris.com/eula.html).

`frames/` contains item-header plaques, separators, window glyphs and search-button
ornaments and profile radios. Caps keep their aspect ratio; plaque centers repeat horizontally.
Search ornaments retain their SVG sources and 4x PNGs, with `currentColor` set to
`#766D4E` (idle) or `#FFE3C7` (highlight). The renderer caches display-size derivatives.
Profile radios likewise retain SVG sources and 4x PNGs: `#9F9D8A` (unchecked)
and `#FFE3C7` (checked), rendered in a 16px frame inside a 24px control at 100%.
The listing eye and refresh retain their SVG sources and 4x PNGs for 16px display frames.
The search-error glyph also retains its source SVG and a 4x PNG for a 16px frame.
Refresh uses the reference's Material UI Refresh glyph (MIT license).
The estimate help glyph uses the reference SVG in a 12px frame. The approximation
symbol uses the Liberation Sans Bold outline at 16px, matching the reference
browser's fallback for the missing Lato glyph. Both retain 4x PNG derivatives.
Liberation Sans is copyright Google (2010) and Red Hat (2012), licensed under
SIL OFL 1.1 (the license text is in `Lato-OFL.txt`).

The current reference set comes from PoE Overlay II 1.68.0. Its relationship to
the original game assets and the applicable redistribution terms remain to be
verified. These third-party images are not covered by Retinue's Apache license.
