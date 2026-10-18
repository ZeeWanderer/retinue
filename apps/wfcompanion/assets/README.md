# Embedded Overlay Assets

`platinum.png` and `ducats.png` are Warframe currency icons owned by Digital Extremes. These exact
copies come from the AlecaFrame 2.6.90 package under
`research/alecaframe/2.6.90/package/web/assets/img/` because they match its overlay reference.

Their DE provenance can be checked independently:

- `platinum.png` is the [`PlatinumLarge.png`](https://wiki.warframe.com/w/File:PlatinumLarge.png)
  art published by the official WARFRAME Wiki.
- `ducats.png` is an older size of the current
  [PublicExport asset](https://content.warframe.com/PublicExport/Lotus/Interface/Icons/StoreIcons/Currency/Ducat.png!00_-49OhiZUiif4NhKrOS8EpA).
  `ExportManifest.json` maps
  `/Lotus/Types/Items/MiscItems/PrimeBucks` to
  `/Lotus/Interface/Icons/StoreIcons/Currency/Ducat.png`.

Shared fonts and their licenses live under [`assets/`](../../../assets/README.md).

`forma.png` is Digital Extremes' Forma art copied from AlecaFrame's public asset CDN. Aleca uses
this same custom image for relic rewards because WFCD relic data does not publish a Forma reward
image.

`relic-parts/` contains universal Prime-part textures published through Warframe Market's static
asset service. Their stable `sub_icons/...` identities are also present in Market item metadata.
The `_128x128` suffix is Warframe Market's stable identity, not an exact-dimensions guarantee;
these PNGs preserve their source aspect ratios. The companion maps all Market identities onto the
unique embedded images, decodes each unique image once, and aspect-fits it at render time. Unique
item images still use the daemon asset cache.

The Warframe icon files are not covered by this repository's Apache-2.0 license. Their use remains
subject to Digital Extremes' [Warframe Content Policy](https://www.warframe.com/en/contentpolicy),
including its non-commercial-use terms.
