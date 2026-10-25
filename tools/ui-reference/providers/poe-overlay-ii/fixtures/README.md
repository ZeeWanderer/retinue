# Currency Images

`currencies.json` records a small sample from GGG's `/api/trade2/data/static`,
with SHA-256 digests for reproducible previews. `currencies.mjs` downloads the
official CDN images into `.cache/ui-reference/poe2/currencies`; images are not
embedded in the companion. Both renderers use these same resources.

This fixture is not the runtime catalog. The daemon refreshes the full catalog
independently of Retinue releases.
