# Native Desktop UI

`apps/wfgui/src/` is the default `wfgui` client. `make gui-dev` and `make gui-prod`
stage the matching Rust executable. All seven page routes have native implementations.
Qt source and tests remain in `apps/wfgui/qt/`; explicitly build them with
`make gui-qt-dev` or `make gui-qt-prod`, which stage `wfgui-qt` instead.
Qt is a temporary reference, not a parallel implementation to maintain. The next
release waits for the native replacement and removal of Qt.

The three-rail shell uses the existing GUI icons and custom title bar. Drag,
double-click maximize/restore, resize edges and window controls delegate to Winit.
Settings is pinned to the navigation bottom. The player panel retains its rank
emblem when collapsed; the full identity remains accessible through its tooltip.
Rail toggles animate for 170ms and can reverse mid-transition.
The 1024x600 minimum follows UI scale; increasing scale grows an undersized window.
Narrow windows hide the activity rail without changing the user's preference.
Explicit market navigation opens it as an overlay drawer when it cannot fit beside
the content. Mouse routing and wheel handling prioritize that drawer.

## Boundaries

- `retinue-ui`: retained composition, painter, Taffy layout, caches, scrolling and transitions.
- `desktop` feature: Winit windows/events/accessibility and Softbuffer presentation.
- `overlay` feature: existing layer-shell, game focus, portals and clipboard.
- App: screens, styles and asynchronous daemon/filesystem work. Daemon retains
  fetching, canonical data and domain computation.

Desktop buffers are opaque. Paint directly into the acquired buffer, repair damage
using buffer age, finish Blend2D work, then present without a conversion pass.
The host accepts only Wayland/X11. Their pinned Softbuffer backends submit XRGB;
the high byte is ignored, so keep Blend2D's opaque `FF` rather than clearing it.
This relies on verified backend behavior beyond Softbuffer's stricter public
zero-padding contract; review it when upgrading Softbuffer or adding a backend.
Painter colours and image storage are BGRA. Unknown age or resize needs full
repair. Paint with the dimensions configured on the backing surface, not a fresh
window-size query: the window can change size before its resize event is handled.
Scoped viewports preserve parent clips and use device-pixel origins;
the pinned Blend2D mask implementation does not support fractional transforms.
UI scaling is baked into layout and glyph size, not applied as a paint transform.
The desktop host prefers six total Blend2D threads, bounded by available CPUs;
headless desktop captures use the same policy. Small companion surfaces keep their
own lower budgets. Cached glow preparation is separate from steady frame rendering.
See [Blend2D mask issues](blend2d-issues.md) for reproductions and local workarounds.

App colors live in `style.rs`; reuse semantic names for ownership, search,
menus and surfaces. Define them with `rgb`/`alpha`, not RGB byte arrays passed to
the BGRA painter. Component readiness is Aleca's `#6e9b6878` background behind
the artwork and `#46d234` inset glow above it. Never tint the artwork itself.
The warning triangle uses Lucide's `triangle-alert`, recolored for the app;
its license is in `apps/wfgui/resources/Lucide-LICENSE.txt`.

Shared Aleca views use its extracted templates and CSS as the visual reference,
not Qt's approximations. Qt is the minimum behavior and feature checklist, not a
limit: use Aleca's behavior and normal desktop expectations to address omissions.
Preserve intentional Retinue changes such as compact
Foundry cards and title-line badges. `alecaframe foundry` in the UI reference
tool renders vendor cards/components for close-up comparisons. Owned backgrounds
and component readiness use inset glows; `Painter::inset_shadow` approximates
the Gaussian edge with queued vector bands. Repeated component glows are painted
once per scale with `RasterImage::render`, held in RAM and reused through Blend2D
image operations. Do not create raster contexts per card or per frame.

Sample transitions at redraw, not on every event-loop wake. Keep one redraw pending
while Winit waits for the compositor; measure the next deadline before painting so
render time does not extend the interval. A late wake must still paint the final
position. Idle windows sleep. Layout changes invalidate geometry; scrolling and
moving a width-capped page translate its existing geometry without rebuilding it.
Paint, hit testing, captures and accessibility must use the same translation.
Keep controls keyboard-operable and expose their semantics through AccessKit.
Do not activate controls from synthesized held-key presses on focus gain.
Keep logical focus separate from its visible indicator. The window owns one
`FocusVisible` modality state; pointer presses hide button rings, non-shortcut
keyboard input restores them, and hover does not change modality. Modifier-only
and synthesized keys do not enable rings. Text fields retain an editing indicator;
inactive windows hide indicators without discarding focus. Programmatic focus
transfers preserve modality, and accessibility focus uses keyboard presentation.
All native controls use `style::focus_ring` or `style::focus_capsule`; do not add page-specific outlines.
This follows the [focus-visible heuristics](https://www.w3.org/TR/selectors/#the-focus-visible-pseudo)
with no automatic mouse-triggered button ring when opening a dialog.
Taffy supplies geometry, not widgets, focus, accessibility or virtualization.
Time-based easing lives in the shared transition module. App compositions use the
shared keyed scene; custom layout/paint remains supported. Do not add a framebuffer conversion or copy without a demonstrated
consumer requirement and a measured cost.

### Layout Evidence

`--report` includes native control/region bounds and draw styles in logical pixels.
`make previews-gui` writes `layout/<page>.json` beside the images, joining these
to Aleca DOM measurements and named Qt source constants. Missing observations
remain absent; source-derived dimensions are not runtime measurements. Use live
captures for artwork, since deterministic fixtures intentionally omit remote assets.
Capture a listing context menu with `--view context-menu --capture-target page.menu`.

### Shared Controls

Use `Ui::search_field` for page and rail searches: collapsed search capsule,
animated focus expansion, immediate growth under text pressure and an inline clear
button that retains focus. Its editor, caret, hit test and capture bounds share
the same inset. Forms, passwords and numeric values remain ordinary fields.

`menu::Layout` supplies one clamped context-menu surface, wrapped heading/metadata
and flat action rows. Foundry and listing menus use the same geometry and paint;
dropdowns reuse its surface and row style. Outside presses dismiss and are consumed.
Arrows/Home/End select actions; Enter/Space activates; Escape and Tab dismiss and
restore invoking focus. Keyboard invocation anchors to the focused control, not
the last pointer location. These follow the
[menu interaction pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/).
Filter popovers retain their separate form-style Tab traversal.

Navigation, including Settings, has no idle filled background. Hover changes the
text/icon and trailing indicator; selection uses the same geometry in white.

### Foundry Controls

Search expands on click/focus, stays open for nonempty text and grows with the query.
Its right edge and toolbar row budget stay fixed; opening it cannot move the grid.
Text that outgrows the field expands it immediately; only opening, shrinking and
closing are animated. Retargeting an animation on each keystroke makes typing lag.
Ctrl+F selects the query; Escape leaves editing without clearing it. The clear
button keeps editing active. Tradeable components open the market book; owned parts
show buyers, missing parts show sellers. Void badges search Relics by reward name.
Right-click opens a viewport-clamped popover for counts, part favorites and related
pages. Unlisted components open those details directly. Popup targets and keyboard
focus follow canonical IDs through snapshot reordering, not stale row indices.
Repeated ingredients also retain their slot occurrence and available count. Use
the daemon's `full_name` for component details/search, not abbreviated recipe labels
or market names: a built part and its tradeable blueprint are different items.
Market and relic navigation use the canonical market/reward identity instead.

For filters, the shared `Popover` owns anchoring, viewport clamping and dismissing presses.
Foundry owns its paired toggle choices, scrollable content and runtime state.
Keep Tab traversal inside while open, reveal focused rows, close on Escape or
actual focus loss, and restore the trigger. A dismissing press must not activate
the card or window control underneath. The initial unfocused-window event is not
focus loss. Input, paint and accessibility use the same popup geometry.
These follow the [dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)
and [toggle button](https://www.w3.org/WAI/ARIA/apg/patterns/button/) interaction
patterns; this lightweight popup is not advertised as a modal dialog.

Filters compose locally with category and search without changing the daemon
snapshot or fetching again. Crafting use comes from the daemon's recipe reverse
index; shards and Incarnon aggregate across owned equipment instances. Incarnon
uses an installed Genesis flag or weapon `skill_tree`, covering native Incarnons
as well as adapters. Catalog compatibility alone is not an installed Incarnon.
Favorites include component favorites. Unknown player rank is not rank zero;
vault filters apply to Prime equipment. Selecting an active choice unsets that row.
Prime Resurgence and Helminth eligibility still need canonical projections.

Desktop clipboard operations use a lazy, bounded worker and retain the Linux
selection owner. [Arboard](https://docs.rs/arboard/latest/arboard/struct.Clipboard.html)
provides Wayland data-control and X11 backends. No clipboard reads at startup or
during painting. Cut deletes only after a successful copy; delayed replies are
discarded if the editor changed. `LineEdit` clamps horizontal overscroll after
resizing/deleting text and reuses its raster when only its position changes.

## Preferences

One app-owned preference model shares canonical-item favorites and UI scale across
pages. They are GUI preferences, not changes to the game's inventory or Favorite
field. Store them in the Warframe config directory's `gui.json`; `--paths-json`
reports the location. Load once at interactive startup; explicit CLI scale is a
temporary override. Fixtures and both capture modes use isolated defaults.

Only user edits enqueue saves. A background writer coalesces edits for 250ms and
flushes on normal close. Merge changed keys under a bounded
[file lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) before atomic
replacement, preserving unrelated changes from other windows. No disk work during
paint/input, no idle polling. Failed loads/saves preserve the existing file, show a
Settings warning and retain pending edits for the next save attempt. Don't put
canonical player/build data in this store; that remains daemon-owned.

## Pages And Contracts

`pages/` owns presentation for Inventory, Mastery, Relics, Market, Build Planner
and the right rail. `widgets.rs` shares field editing, dropdowns, scrollbars,
modal focus, tooltips and accessibility. Paint and input consume the same clipped
geometry; an open modal excludes underlying controls, including the other rail.

Use the shared [composition layer](ui-composition.md) for layout, retained paint,
clipping and presentation changes. App `Ui` remains a custom-layout facade, not a
second renderer. Pair `begin_scope`/`end_scope` around keyed collection rows and
finish the layout transaction before input or paint. Specialized artwork and editors
use direct custom paint; never wrap a page in an offscreen bitmap just to compose it.
Keep data fetching and resource preparation outside painting. Preserve visible-only
preparation, stable control IDs and damage bounds. Verify geometry, pixels and
scrolling cost before replacing a rendering path.

The page session multiplexes bounded requests with player push updates. Activity
uses the existing 60-second query refresh; the local protocol does not offer a
world-state subscription. Reject failed subscriptions rather than silently losing
updates. Request identity rejects superseded search/filter replies. Never replay
account/group mutations automatically after a timeout or disconnect.

Price-based grids fetch all candidates, display cached values first, and reorder
once the current batch settles. Preserve the visible item anchor. Image arrivals
damage their draw bounds, translated through the same scroll viewport as input.
Decode failures/recoveries update the existing diagnostics registry only on change.

One topology renderer handles equipment, imported builds and calculated group
loadouts. Slot IDs and costs come from daemon projections. Frame variants come
from `mod_variant`, not the item name. Expanded cards retain endpoint-sized image
derivatives; frame backgrounds aspect-cover, side lights stretch. Markdown notes
are laid out below the topology and accept only HTTP(S) links. The daemon owns
group revisions, source fingerprints, current polarities and plan computation.

## Foundry And Images

Foundry consumes the daemon's canonical view and player/asset events. Filtering
is local; refresh preserves the visible item and its position. Uniform grid
geometry limits painting and hit testing to visible rows. One overscan row feeds
image preparation; the latest viewport replaces unsent asset/preparation work.
Category capsules transition together; toolbar wrapping reserves the widest
selection per row so animation cannot move the grid. Sampled geometry also drives
hit testing and search placement. New selections retarget the current animation.
Wheel steps use 60 logical pixels, matching
[Kirigami's default](https://api.kde.org/qml-org-kde-kirigami-wheelhandler.html) of
20 pixels times three scroll lines. Physical touchpad deltas remain unscaled and
unsmoothed.

`retinue-ui::thumbnails` validates source hashes, decodes bounded images and uses
Blend2D Lanczos resampling at the final physical size. One preparation worker owns
24 MiB of decoded-source retention and 32 MiB of derivative retention. These are
idle-cache budgets, not limits on the visible working set. Immutable image handles
are shared with the painter; weak lookup preserves reuse of active images after
LRU eviction. Expired lookup metadata is reclaimed without retaining image bytes.
Cache accounting includes handles still held by a frame;
an old image remains visible until its refreshed replacement is ready. Source
files persist in the daemon cache. Preparation publishes RAM hits first, then each
completed image; one coalesced mailbox bounds notifications. Only affected cards
are repainted. Cancellation drops stale results, not images already on screen.

Catalog derivatives also persist under `gui/derivatives/native-v1`. Keys include
source identity, source hash, physical bounds, crop and resampling recipe. PNG
writes follow visible preparation; embedded UI icons stay RAM-only. The Qt cache
has a separate namespace because its pixel recipe differs. There is no disk size
cap: obsolete source revisions are removed after a replacement is stored. Atomic
writes and a cache-generation lock prevent old work from restoring cleared files.
Corrupt or missing disk entries fall back to preparation and can be rewritten in
the same session; a previous successful write must not suppress repair. Settings
counts and clears both derivative namespaces; memory controls include glyphs.

Keep file access, decoding and resampling outside paint/input handlers. Scroll
repaints the viewport, not the entire window; model changes damage affected visible
cards. Offscreen changes do not repaint. Buffer-age repair also accumulates damage
missed by recycled buffers. Foundry reuses isolated card rasters with local damage
repair, not copies of the presentation buffer. See [incremental rendering](ui-invalidation.md)
for invalidation, active residency and full-replay equivalence requirements.
Painting looks up derivative keys directly. Do not construct preparation requests
or recompute disk identities there. Source-file integrity still uses the daemon's
SHA-256 digest on the preparation worker; a faster table hasher is a separate choice.

## Verification

```sh
make test-gui
make test-gui-live # read-only session check against the running daemon
make previews-gui GUI_PREVIEW_SCALES='100 125 200'
make bench-gui GUI_BENCH_OUTPUT=/absolute/path/to/animation.json
dev/bin/wfgui --page settings --screenshot settings.png --ui-scale 125
dev/bin/wfgui --page settings --fixture --screenshot storage.png --capture-target settings.storage
dev/bin/wfgui --screenshot foundry.png --size 2560x1440 --ui-scale 125
dev/bin/wfgui --screenshot card.png --capture-target foundry.card --search "Ash Prime"
dev/bin/wfgui --screenshot filters.png --capture-target foundry.filters --ui-scale 125
dev/bin/wfgui --screenshot component.png --capture-target foundry.component --search "Ash Prime"
dev/bin/wfgui --screenshot scroll.png --scroll 600
dev/bin/wfgui --fixture --window-screenshot desktop.png
dev/bin/wfgui --fixture --screenshot title.png --capture-target titlebar
dev/bin/wfgui --fixture --screenshot collapsed.png --left-collapsed --right-collapsed
dev/bin/wfgui --fixture --screenshot transition.png --capture-animation both --capture-time-ms 85
dev/bin/wfgui --fixture --screenshot categories.png --capture-target foundry.toolbar --capture-animation category --category 3 --capture-time-ms 85
dev/bin/wfgui --fixture --page relics --screenshot relic-tabs.png --capture-animation category --capture-category-from 0 --category 4 --capture-time-ms 85
dev/bin/wfgui --fixture --screenshot search.png --capture-target foundry.toolbar --capture-animation search --capture-time-ms 100
```

Offscreen captures use live data unless `--fixture` is supplied. Window captures
use the actual presentation buffer and wait for the page, images and visible rail
to settle, including error states. Targets:
`window`, `titlebar`, `player`, `left-rail`, `center-rail`, `right-rail`, `settings.daemon`,
`settings.storage`, `settings.scale` (now in the title bar), `foundry.grid` and
`foundry.card`, `foundry.toolbar`, `foundry.filters` and `foundry.component`.
The component target opens the first fully visible component. `--show-filters` opens the
panel in whole-window captures. `--report FILE` writes cache data or
Foundry/image diagnostics, including RAM/disk hits, resampling and cache warnings.
Captures do not clear caches.
Other pages expose named regions and control IDs in their capture reports. Use
`--view` for a page mode/tab, `--select` for equipment/group/build selection and
`--hover CONTROL_ID` for the expanded mod state. `--market-item NAME` captures a
real order book without posting or changing orders.
Market's `--view edit-order` and `delete-order` open a draft or
confirmation only; `--select` accepts an order ID or item name. Capture the dialog
with `--capture-target market.dialog`. No capture confirms an account mutation.

Tests cover scaling, focus/confirmation, cache boundaries, frame limits, clipping,
buffer age, interrupted rail transitions and isolated X11 window/input/resize/
collapse/shutdown through Xvfb, streamed updates, stable scroll anchors, source
replacement and recycled-buffer grid repainting. Minimum desktop size is 960x600
logical pixels.
Set `GUI_FOCUS_CAPTURES` to a directory when running `make test-gui` to
retain scaled keyboard/pointer focus comparisons from the render tests.
`GUI_FILTER_CAPTURES` retains selected/scrolled filter closeups at 100%, 125% and 200%.
`GUI_INTERACTION_CAPTURES` retains search and component closeups at those scales.

The opt-in animation benchmark measures layout and completed painting at
1080p/100% and 1440p/100%/125%. It uses the dev profile,
deterministic fixtures and warm alternating left-rail transitions. Reports include
raw samples and p50/p95/max; they exclude compositor presentation and input latency.
No timing thresholds run in CI. Keep background load comparable between runs.

For scrolling, select `GUI_BENCH_CASE=foundry_scroll_benchmark`; add
`GUI_BENCH_LIVE=1` to use cached player data and images. It compares whole-window
and viewport repaint with the same warm assets, reporting image memory and frame
timings. Fetching/preparation are outside the timed section. Set
`GUI_BENCH_PROFILE=release` for optimized timings; the default remains `test`.
Compare matching profiles and compiler flags.

`GUI_BENCH_CASE=foundry_image_benchmark` measures cold preparation, restart disk
hits and RAM hits using cached player data and an isolated temporary derivative
cache. It reports first catalog image, all-images-ready and deferred-write times.
No daemon fetch or compositor time is included; user caches are not cleared.

Remaining migration work includes Foundry cross-page actions,
IME, selectable non-input text, context menus, saved window
preferences and the other pages. Favorites and UI scale are persisted. Native Settings
currently refreshes cache status on request; its Available
label describes that check, not a persistent connection. Do not claim full Qt
parity or better performance than Qt from these ports.

Windowing references: [Winit](https://docs.rs/winit/0.30.13/winit/),
[Softbuffer](https://docs.rs/softbuffer/0.4.8/softbuffer/struct.Buffer.html),
[AccessKit](https://docs.rs/accesskit_winit/0.34.1/accesskit_winit/).
