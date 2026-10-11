# Asset Management

Game-content textures must update with data, not with the application binary. Stable presentation
assets needed before any catalog or network access remain embedded.

## Upstream Model

AlecaFrame's downloaded catalog stores an `imageName` on items and components.
`Misc.GetFullImagePath` converts that value to a CDN URL, and its HTML uses the URL as a CSS
background image. AlecaFrame does not implement a separate texture downloader in its client;
Chromium performs the request and browser caching.

WFCD `warframe-items` uses the same useful model. Catalog records contain `imageName`, and images
are available at:

```text
https://cdn.warframestat.us/img/${imageName}
```

The catalog already expresses reuse:

- Prime Chassis, Neuroptics, Systems, Barrel, Receiver, Stock, Blade, Handle, and similar
  components point to shared generic filenames.
- Relics reuse one image per era and refinement, such as `RelicAxiA.png` through
  `RelicAxiD.png`.
- Warframes, weapons, and other inspectable items point to item-specific filenames.

Do not duplicate those classifications in application code. Resolve the catalog's exact
`imageName`; normal cache keys naturally reuse generic textures and accept new unique items after
a catalog refresh.

## Asset Classes

Embedded assets:

- UI chrome required when the daemon or network is unavailable.
- Roboto and stable currency symbols such as platinum and ducats.
- A small missing-image placeholder.

Catalog assets:

- Item, component, set, relic, mod, resource, frame, and weapon textures.
- Fetched lazily from trusted catalog descriptors.
- Never bundled wholesale; the current WFCD image tree is hundreds of megabytes.

Player rank cards use the constrained `mastery` source (`<rank>.webp`). They share the daemon
cache and validation path used by catalog assets; Qt clients include WebP decoding.

Embedding is based on startup or offline necessity, not on how often a texture happens to repeat.
A shared prime-component image can still be a catalog asset because one cached file serves every
item that references it.

## Ownership

`retinue-daemon` owns:

- Catalog identity to `imageName` resolution.
- Source URL policy, HTTP revalidation, download coalescing, and persistent files.
- A small metadata index containing source URL, ETag or Last-Modified value, content type, digest,
  size, and last successful fetch.
- Stale-on-error behavior and cache pruning.

Native graphical clients own:

- Decoding bytes into renderer-native images.
- In-memory caches keyed by content digest and requested render size.
- Layout, placeholders, transitions, and repaint invalidation.

Consumers must not issue independent CDN requests. That would duplicate downloads, bypass daemon
policy, and produce inconsistent offline behavior. The daemon must not keep Blend2D, GPU, or UI
objects.

## Cache Contract

Store downloaded bodies under:

```text
$XDG_CACHE_HOME/retinue/games/warframe/assets/objects/<sha256>.<extension>
```

Map canonical source URLs to those objects in one daemon-owned metadata file. Download to a
temporary file, enforce a size limit and accepted image content type, hash the body, then rename
atomically. Identical bodies from different URLs collapse to one object.

An asset request returns a descriptor containing at least:

```text
asset id, local path, SHA-256 digest, media type, byte size
```

The local path is suitable because the companion and daemon share the host filesystem. Keep the
descriptor versioned so a byte-stream response can be added later without changing item entities.
Missing or failed assets return a normal unavailable result; they do not fail the surrounding
market, player, or catalog request.

Use conditional requests when the source supplies ETag or Last-Modified. Serve a cached object
immediately when fresh, and retain the previous valid object if revalidation fails. Coalesce
concurrent requests for the same canonical URL.

## Catalog And Fallbacks

Preferred item source is the daemon-managed WFCD item catalog because it covers both tradable and
non-tradable game content and exposes component relationships. Warframe Market icon or thumbnail
paths may be fallback metadata for tradable items, but they are not a complete game catalog.
AlecaFrame item URLs are research evidence, not a catalog dependency; rank cards are the explicit
presentation-only exception documented above.

Refresh of the WFCD catalog makes newly added items resolvable. Their textures are fetched only
when a visible scene requests them. No application release is needed when a new item uses existing
catalog fields.

## Rendering Policy

Relic reward cards should request visible reward and set-component assets as one batch while the
existing loading scene is active. Publish the completed card scene once text, market data, and
required textures are ready; timeout to stable placeholders rather than adding several layout
stages.

Relic recommendations should prefetch only the visible page plus a small look-ahead window. Scroll
or filter changes cancel obsolete low-priority requests. Keep card geometry stable while textures
arrive.

Decoded images are renderer caches, separate from the daemon's encoded-file cache. Overlay scaling
and shaped image fills stay in Blend2D; do not create resized Rust pixel buffers during scene
drawing. Blend2D static-scene caching remains useful after decode: static card art can be
rasterized once, while a changed price or selection invalidates only the affected scene region.

Qt widgets load catalog and embedded images through `wfgui::cachedThumbnail` in `image_cache`.
Keys include source digest (path for embedded assets), target bounds, and device-pixel ratio.
Individual widgets must not decode full-size pixmaps or maintain separate thumbnail caches.
Accept daemon descriptors through `acceptThumbnailAsset` before updating models. Its ordered
background writer owns source freshness; decode jobs only read identities. Clearing derivatives
retains source markers so delayed jobs cannot revive superseded images.
