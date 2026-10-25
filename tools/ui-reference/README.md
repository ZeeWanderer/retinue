# UI References

Render provider interfaces in headless Chromium. Shared browser and video helpers
live in `shared/`; each provider owns its bootstrap, fixtures and source discovery
under `providers/`. Extracted packages and generated evidence remain ignored.

Image renders include an HTML snapshot beside the image: final DOM, CSSOM styles
and form values, with scripts removed. Source assets remain linked locally.
Warframe snapshots capture the component tree before screen-position compositing;
each scene has its own filename.

Screenshots retry Chromium's transient capture failure twice. Failed scenes do
not stop the remaining resolutions or games. The final summary lists failures;
the command exits nonzero if any remain.

```bash
make ui-reference-setup
node tools/ui-reference/render.mjs --list
npm --prefix tools/ui-reference test
```

## AlecaFrame (Warframe)

```bash
./scripts/setup-ui-reference alecaframe
node tools/ui-reference/render.mjs alecaframe market-orders
make previews-wf PREVIEW_MEDIA=image PREVIEW_SETS=reference
```

Setup queries Overwolf's installer metadata and reuses or downloads the latest
compatible OPK under `research/alecaframe/<version>/`. For offline setup, append an
OPK or extracted extension directory to the setup command.

Scenes: `desktop`, `foundry`, `relic-rewards`, `relic-suggestions`, `relic-planner`, `market-orders`.
Foundry renders the vendor Vue template with synthetic inventory, including card
and component close-ups. Rewards and suggestions render vendor components; planner and market use
reconstructed fixture markup with vendor styles. Do not treat those as live-app
screenshots.

### Desktop Pages

```bash
make previews-gui
make previews-gui GUI_PREVIEW_PAGES='foundry relics' GUI_PREVIEW_SCALES=125 GUI_PREVIEW_LIVE=1
```

This renders all native pages at 100%, 125% and 200%. Foundry, Mastery, Inventory
and Relics also render the packaged Aleca Vue template/CSS and a side-by-side pair.
Reference fixtures test composition, not live account contents. Native fixtures
are deterministic; `GUI_PREVIEW_LIVE=1` uses read-only daemon data instead.
Outputs are under `previews/warframe/gui/<resolution>/<scale>/`, separated into
`native`, `reference`, `side-by-side` and `layout`. Unsupported pairs are not emitted.
`GUI_PREVIEW_RESOLUTIONS` defaults to `2560x1440`.

`layout/<page>.json` pairs measured native regions and Aleca DOM geometry/styles
with source-derived Qt dimensions and file references. Measurements use logical
pixels; native colors use CSS `#RRGGBBAA`. These explain differences, not an
automatic pixel-parity verdict. Fixtures check composition; use live captures
to inspect resolved artwork. The crosswalk is in `gui-layout.mjs`.

`ALECA_LAYOUT_SIZE`, `ALECA_LAYOUT_DPI`, `ALECA_LAYOUT_OUTPUT_DIR` control direct
rendering. Animated suggestions use `ALECA_LAYOUT_MEDIA=video`; FFmpeg needs
`libvpx-vp9` (`UI_REFERENCE_FFMPEG` overrides its executable).
`make previews` (or `previews-all`) renders every supported game. Use `previews-wf`
or `previews-poe2` to select one. Warframe references are written to
`previews/warframe/<resolution>/reference/`.

## PoE Overlay II (PoE2)

The original feature is **Evaluate**: window/renderer `ingame-evaluate`, state
`evaluate`, configuration `evaluateConfig`. Its native counterpart is
`apps/poe2companion/src/evaluate/`; shared comparison fixtures are
`apps/poe2companion/tests/fixtures/evaluate-*.json`. Use these names when tracing
UI parity. Trade services remain separate from the window in both implementations.

```bash
node tools/ui-reference/render.mjs poe-overlay-ii evaluate
node tools/ui-reference/render.mjs poe-overlay-ii evaluate --state results \
  --width 2560 --height 1440 --scale 1.25 --output research/retinue/providers/evidence/evaluate-results
make previews-poe2 PREVIEW_MEDIA=image PREVIEW_SCALE=1.25
```

The adapter runs the original 1.68.0 components, styles and assets; only application
services and data are mocked. It defaults to the extracted `dist` directory under
`research/retinue/providers/extracted/app-poe-overlay-ii-1.68.0/`. Use `--source` for
another location. Version-specific module IDs fail visibly on incompatible bundles.

`--fixture` accepts JSON Redux slice overrides; `--selector` adds a component
capture. `--scale` scales the UI through device pixel ratio without changing the
output resolution. External requests are blocked. The same rare-item and listing
fixture feeds the native and reference previews; prices are synthetic.

`--state`: `ready`, `predicting`, `estimate`, `estimate-error`, `searching`, `fetching`, `results`,
`listing-preview`, `empty`, `search-error`, `fetch-error`, `cooldown`, `gem-auto`. States use the
original controls and reducers with deterministic backend replies. Each JSON
report records the triggering actions and intermediate layout bounds. Fetching,
fetch-error and cooldown have matching native previews. The automatic-gem scene
remains reference-only.
`listing-preview` is shared by both providers and opens the second result's
read-only item preview. Each listing fixture carries its own item data.

`--exercise FILE` runs an ES module's default async function with the Playwright
page before capture and saves its returned evidence to `<name>.interactions.json`.
For source-backed filter and keyboard behavior checks:

```bash
node tools/ui-reference/render.mjs poe-overlay-ii evaluate \
  --exercise tools/ui-reference/providers/poe-overlay-ii/attribute-interactions.mjs
node tools/ui-reference/render.mjs poe-overlay-ii evaluate \
  --exercise tools/ui-reference/providers/poe-overlay-ii/search-interactions.mjs
node tools/ui-reference/render.mjs poe-overlay-ii evaluate \
  --exercise tools/ui-reference/providers/poe-overlay-ii/profile-interactions.mjs
```

Native preview fixtures accept `"profile": "bases"`, `"nomods"`, `"allmods"` or
`"fuzzy"`, provided the item's `profiles` list contains that choice. Items carry
`default_profile`; modifier rows carry `quick_selected`, `exact_selected` and
`base_selected`.
`profile-ranges.mjs` checks the original query builder against the shared daemon
and companion range fixtures. `profile-ranking.mjs` checks modifier scores and
selection; `profile-defaults.mjs` checks rare, magic, fractured and normal-item
defaults in the original UI. `property-ranking.mjs` verifies quality normalization,
DPS, property scoring and selection against shared daemon fixtures. `motion.mjs`
records computed transitions and animation keyframes without adding animations
to Retinue. Run these with the same `--exercise` option.
`parser-oracle.mjs` runs the original clipboard parser and query builder against
shared modifier fixtures. `evaluation-behavior.mjs` checks prediction independence
and automatic-search categories; `results-controls.mjs` checks the trade link and
refresh glyph. `results-refresh.mjs` verifies that refresh leaves existing rows
visible while the new search is pending. `search-cache.mjs` (with `--state results`)
checks revisits, canonical query identity, retention, eviction and automatic retry
behavior. These require the extracted provider, not live trade requests.

Both sets use `previews/poe2/<resolution>/<companion|reference>/<scene>.<extension>`.

`make previews` also writes compact still comparisons to
`previews/<game>/<resolution>/side-by-side/<scene>.png`. Original is left, Retinue right;
transparent margins are removed without resizing either image. Comparisons require
both providers to render the same scene successfully; unpaired scenes stay in their
provider directory. Crop bounds and source paths are saved in adjacent JSON.
Failed renders never reuse old images in a comparison.
`PREVIEW_LAYOUT=screen` keeps full-screen output instead, including Warframe
fixture backgrounds. Videos are unchanged.

To compare two existing transparent renders:

```bash
node tools/ui-reference/compare.mjs --reference original.png --companion native.png --output comparison
```
The initial PoE2 scene is `evaluate`; other names follow `evaluate-<state>`. Reference
outputs are `.png`, `.html`, `.json`; native outputs are `.png`, `.json`.
`--output` supplies the filename prefix, not a directory. `--selector` adds
`<name>-element.png` for close inspection.

PNGs have the requested canvas size. Reference HTML retains the actual component
tree before compositing; reports include scale and placement. The renderer honors
the provider's window-width request instead of stretching its controls across the
screen. Preview placement is deterministic, not a recovered Windows position:
the provider retains the user's window location.
