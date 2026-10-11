# Overwolf And AlecaFrame Overlay Research

Research snapshot: AlecaFrame 2.6.90, Overwolf GEP 308.0.14 and documentation, KDE Plasma 6.7,
and XDG desktop portal documentation inspected in July 2026. Decompiled AlecaFrame
sources are retained under ignored `research/alecaframe/2.6.90/`.

See `docs/developer/alecaframe-overlay-catalog.md` for a field-by-field catalog of packaged
in-game windows and local implementation status. This document focuses on mechanism, ownership,
and safety boundaries. Detailed player-payload and texture-cache decisions live in
`docs/developer/player-data.md` and `docs/developer/assets.md`.

## Findings

AlecaFrame is one product with two presentation surfaces:

- A normal desktop application for inventory, mastery, relic planning, trading,
  configuration, and history.
- Several transparent in-game windows for relic rewards, relic recommendations,
  rivens, notifications, and completed trades.

AlecaFrame does not implement the low-level in-game overlay itself. Its manifest marks
those windows `in_game_only`, targets Overwolf game ID `8954`, and asks Overwolf to open
or close them. Overwolf owns game detection, graphics integration, window composition,
and hotkeys.

Overwolf's normal overlay path is render-API injection. Its public APIs expose game
registration and explicit injection, while diagnostic logs describe injected DLLs and
render hooks. Supported capture paths include D3D9, D3D11, D3D12, and Vulkan. Overwolf
also has an out-of-process overlay path for selected games; no public evidence found that
Warframe uses it. Do not assume that it does.

This explains why an ordinary always-on-top desktop window is not equivalent. A normal
X11/XWayland window can disappear below a fullscreen game, enter task switching, appear
over unrelated applications, and lose ordering when focus changes.

## AlecaFrame Data Flow

AlecaFrame combines three distinct acquisition paths:

1. Overwolf Game Events Provider (GEP) supplies Warframe `match_info`, including full
   inventory and highlighted items. AlecaFrame forwards inventory JSON into its C# data
   model. This data is supplied by Overwolf; it is not reconstructed by AlecaFrame OCR.
   The retained native provider confirms that Overwolf obtains both values through an external,
   read-only game-memory scanner.
2. AlecaFrame listens to Warframe debug output through the Windows DBWIN protocol.
   `OCRHelper` creates `DBWIN_BUFFER`, `DBWIN_BUFFER_READY`, and `DBWIN_DATA_READY`, filters the
   sender PID, and forwards text to `EELogProcessor`. `Got rewards` waits about 650 ms, then
   requests a Warframe screenshot. This is not `EE.log` file tailing.
3. AlecaFrame crops reward-name regions, uploads those crops to its own OCR service,
   matches returned labels against known items, batches price lookups, then opens its
   relic overlay.

AlecaFrame 2.6.90's client exposes its OCR wire protocol but not its model. It splits each
reward into two line images, skips low-edge-content first lines, and posts concatenated JPEG
bytes to `https://api.alecaframe.com/ml/submitImage` with a `ReqDesc` header containing crop
lengths. Returned labels are stripped of punctuation and numeric artifacts, joined, filtered
for short false-positive tokens, then matched exactly, by contained item name, or by
Levenshtein distance. The endpoint appears unauthenticated in the client, but is undocumented
AlecaFrame-owned infrastructure. Keep it as research evidence only; do not make companion OCR
depend on it without service-owner permission and a documented stability/privacy contract.

Reusable local heuristics from that path are: normal and legacy crop geometry, a special
2560x1600 crop correction, one delayed recapture after UI-detection failure, rejection of dark
captures, OCR artifact cleanup, and catalog-based fuzzy resolution. AlecaFrame's private model
cannot be recovered from client code. `wfcompanion` therefore keeps screenshots local and uses
Tesseract plus daemon-owned catalog resolution.

Relic reward enrichment includes item name, ducats, owned count, set completion,
vaulted/favourite state, item price, and whole-set price. Price requests are batched by
item name and set name. AlecaFrame's current client sends them to its own price service
and caches results for 15 minutes. Its detailed order view queries Warframe Market v2
`/orders/item/{slug}` directly with PC/crossplay headers, filters in-game English orders,
then sorts sells ascending and buys descending by `platinum / perTrade`.

AlecaFrame also downloads `https://cdn.alecaframe.com/warframeData/json.zip`. Its `json/`
directory follows the MIT-licensed [WFCD `warframe-items`](https://github.com/WFCD/warframe-items)
catalog: item categories, localization, component recipes, quantities, ducats, drops, relic
rewards, vault dates, images, stats, and wiki metadata. Parent records contain component arrays;
AlecaFrame builds each component's `isPartOf` link while loading them. This is the set graph used
by relic cards. The archive's `custom/` directory adds AlecaFrame-specific crafting, build,
riven, shard, patch, theme, and image data.

Production code must source reusable catalog data from WFCD, not AlecaFrame's CDN. Warframe
Market `/v2/items` already supplies ducats for tradable rewards. Full set rendering needs a
daemon-owned WFCD item catalog so one update supplies the complete graph without per-card HTTP
requests.

Important consequence: market integration alone cannot produce a relic overlay. It also
needs a reliable reward-screen trigger and reward-name recognition. Full inventory
metadata needs a separate player-data source; OCR only sees the current screen.

## Responsibility Model

Current ownership:

- `wfcompanion` owns process discovery, Proton paths, DBWIN observation,
  capture/OCR orchestration, daemon connection, overlay visibility, layout, and animation.
- `retinue-daemon` owns canonical worldstate, catalogs, player snapshots, market cache, request
  batching, persistence, and query projection. It never renders GUI or terminal output.
- `wfcli` owns terminal commands and formatting. Companion commands manage process lifecycle and
  diagnostics; they do not orchestrate visual scenes.

Overlay-local state includes manual hide/show, placement, active animation, and display cache.
Daemon-owned state includes market quotes, canonical item identities, player observations, and
persisted values shared by multiple clients. Additional native interfaces must consume these
contracts without moving their navigation or window state into the daemon.

## Wayland Overlay Decision

Use a native Wayland layer-shell client on KDE. Current implementation uses Rust
`smithay-client-toolkit` and `zwlr_layer_shell_v1` directly; Qt/QML is not required. Do not
use an X11/XWayland top-level window as production overlay.

Required behavior:

- Create a transparent `LayerOverlay` surface with no exclusive zone.
- Request no keyboard focus and use an empty pointer input region for passive click-through
  behavior.
- Keep one XDG Global Shortcuts session for the companion process. Portal bindings are
  session-scoped and the API has no context-specific enable/disable operation; repeatedly closing
  and recreating sessions loses activations on KDE. Gate received activations on a visible
  interactive scene over focused Warframe instead. `Ctrl+Tab` gives the full surface a pointer
  input region so KWin can release Proton's relative-pointer lock and expose the system pointer.
  The second activation, focus loss, scene closure, close button, or outside click restores the
  empty region. All layer surfaces use zero compositor margins; status-panel spacing is rendered
  inside its small surface. Contextual surfaces additionally use layer-shell exclusive zone `-1`
  so panel reservations cannot leave an uncovered screen-edge strip. Pointer leave also resets
  interaction state so a later activation cannot inherit stale capture state. Do not use an X11
  key grab.
- Relic recommendation interaction supports close-button hover/click and wheel scrolling through
  complete two-card rows. Live KDE/Proton validation confirmed wheel input reaches the interactive
  Wayland surface without reaching Warframe. Recommendation and reward cards do not imply game
  actions.
- Render only after state changes. Avoid a continuous repaint loop. Preserve buffer
  damage so static overlays do not redraw the whole output every frame.
- Hide scene pixels as soon as Warframe loses focus. The implementation keeps the layer role
  mapped with one transparent static buffer after first use because KWin can fail to present a
  detached and remapped role; the empty input region remains click-through.

Layer shell places a surface above fullscreen windows but does not target one application.
Proton/XWayland gating uses `_NET_ACTIVE_WINDOW`, then requires the observed game PID or exact
Steam class `steam_app_230410`. Live validation used fullscreen Warframe and an unrelated
fullscreen application; only Warframe mapped the overlay. Native Wine Wayland gating would need
compositor-owned identity:

1. A small KWin script observes `workspace.windowActivated`, window removal, geometry,
   and session state.
2. It sends active-window PID and geometry to `wfcompanion` over session D-Bus.
3. Companion compares that PID with the observed Warframe process tree/Steam app identity.
4. Overlay exposes scene pixels only when the active window is confirmed Warframe and user
   visibility is enabled.

Never use "active fullscreen window" as identity. Match positive Warframe evidence:
observed PID/process ancestry first, Steam app ID `230410` or cgroup second, and exact
window metadata only as fallback. Verify actual XWayland PID metadata against a live
Proton session before implementation is accepted.

Do not render inside a KWin `SceneEffect`. A compositor plugin has a larger failure blast radius,
and the tested prototype produced a full black screen. KWin should report window state; an
external layer-shell client should own pixels.

## Relic And Market Pipeline

Implemented data flow:

1. Receive `Got rewards` from Warframe's debug output and debounce duplicate events.
2. Capture Warframe directly through KWin after UI stabilization.
3. Detect squad reward count and crop resolution-specific reward-name regions.
4. Run Tesseract locally and clean common OCR artifacts.
5. Resolve noisy labels against daemon-owned item metadata.
6. Resolve quotes, ducats, player ownership, set metadata, and visible assets through daemon
   services.
7. Render complete cards on a passive Wayland surface.
8. Hide on timeout, Warframe focus loss, or explicit companion hide.

An XDG ScreenCast/PipeWire window session remains a possible capture backend when it provides a
clear reliability or portability benefit. Capture backends must keep screenshots local by default
and must not continuously record the desktop.

### Proton Debug-Output Bridge

Wine's current `OutputDebugStringA` implementation first raises the normal debugger exception.
When no debugger handles it, Wine opens `DBWIN_BUFFER`, waits on `DBWIN_BUFFER_READY`, writes the
Windows PID plus up to 4091 bytes, and signals `DBWIN_DATA_READY`. This matches AlecaFrame's
listener exactly.

`wfcompanion` builds a small PE listener and runs it with the detected Proton `wine64` and prefix.
The native process reads bounded PID/text records from helper stdout. This works in both Steam
wrapper and standalone modes because both discover the active Warframe process environment;
neither mode attaches a debugger or injects code. An isolated Proton 10 test delivered a known
`OutputDebugString` message through this path. `wfcompanion` does not tail `EE.log`: delayed file
flushes produce stale duplicate lifecycle events.

AlecaFrame's live overlay path also uses DBWIN. Its `EELogProcessor` name describes the message
format, not file acquisition. Direct `EE.log` reads in AlecaFrame 2.6.90 are limited to username
and login-state checks plus a developer fake-log replay path.

### Relic Geometry And UI Scale

AlecaFrame 2.6.90 positions its reward window from game logical dimensions in
`package/web/assets/js/relicOverlay/main.js`:

- reward width is `1000 * game_height / 1080`;
- top is `630 * game_height / 1080`;
- left is centered, then offset left by `15 * monitor_DPI`;
- default layout adds a 420-pixel side column and 60 pixels of height;
- height is `(295 + extra_height) * monitor_DPI / zoomMultiplier`.

At 1920x1080 and DPI 1 default window bounds are `(445, 630, 1420, 355)`. After 15-pixel `main`
margin, 2-pixel border, 7-pixel holder padding, 16% footer, and 400-pixel sidebar, card-row bounds
are `(469, 654, 972, 256)`. `wfcompanion` uses the reward and footer geometry in a content-sized
shell. It uses one full-output layer surface for contextual scenes; completed scenes use static SHM
content, while relic loading has a bounded 30 FPS animation. Renderer caches one complete static
frame per scene and copies it into the next SHM buffer; loading frames redraw only the pulse. Scene,
output size, or scale changes replace the cache. Interaction state and recommendation scroll offset
are also part of the static-frame cache key, so pointer movement redraws only when hover state
changes.

Card markup lives in `package/web/relicOverlay.html`; row proportions live in
`package/web/assets/css/relicOverlay/base.css` as `1.05fr 0.77fr 0.8fr 1.5fr`. Rows are name;
platinum/vaulted/favourite/ducats; crafted plus owned/required; set components plus set price.
Keep these slots stable while fields are added. `.relicHolder` centers flex children and `.relic`
caps each child at 25%, so the outer shell does not change with squad size; one to three cards are
centered inside the same card row.

`OCRHelper.cs::GetRelicCountNew` detects squad size before OCR. It crops the narrow horizontal band
containing reward-card bottom borders, checks paired strips at expected 4-, 3-, 2-, then 1-card
positions, and accepts strips containing a long, thin, low-variance color run. The older fallback
applies a Laplacian filter and tests edge luminosity at similar positions. `wfcompanion` ports the
new border-line heuristic and retains exhaustive 1-4 layout OCR as fallback.

Reward overlay closure is timer-driven, not tied to a mission-transition log event. AlecaFrame
waits up to 650 ms after `Got rewards` for game UI stabilization, captures, then opens the overlay.
If UI recognition fails, it waits 1.5 seconds before one recapture. `wfcompanion` retains both
game-facing timings and likewise opens its loader only after successful capture.

`OCRHelper.cs::DoScreenshotRequestWork` records the capture-work start time.
`GetRelicWindowData` returns 14.5 seconds minus whole elapsed seconds; JavaScript truncates that
result again before scheduling hide. `relicOverlay/main.js` closes the Overwolf window 1.5 seconds
after hide so analytics and ads can observe closure, and its independent 16.55-second timer is a
safety fallback if data setup never schedules the accurate timer. Those latter timings are
Overwolf lifecycle details, not game behavior. `wfcompanion` keeps a persistent native surface, so
one 15-second deadline from loader display is sufficient; OCR and prices produce one final card
update. `InitMapping for all devices with bindings` closes AlecaFrame's separate
relic-recommendation overlay.

AlecaFrame opens relic recommendations on
`ThemedProjectionManager.lua: LoadingCompleteEnd`, except within one second of
`UIConsoleTrigger::Open()`. It waits 750 ms, OCRs the top-left era selector, retries once after
one second, then displays ranked owned relics. Aleca ignores every close marker received within
500 ms of opening and extends that guard to 3.5 seconds when recommendations follow a relic reward.
`wfcompanion` follows these trigger, close-guard, and OCR timings. Recommendation scenes have no
timeout; Warframe permits the selection screen to remain open indefinitely.
Daemon owns WFCD reward tables, player-inventory joins, cached Market quotes, and ranking;
companion owns capture and rendering.

Game UI scaling affects OCR crops separately from overlay DPI. AlecaFrame reads Proton-prefix
`AppData/Local/Warframe/EE.cfg` in `csharp/.../Utils/Misc.cs`:

- `Flash.FlashDrawScaleMode=MSM_CUSTOM` uses `Flash.FlashDrawScale`;
- `MSM_MATCH_SCREEN` selects legacy crops;
- other values select full-screen crops.

`csharp/.../OCRHelper.cs::getOCRsettings` scales crop tops/bottoms around normalized screen center
(`0.5 + scale * (coordinate - 0.5)`) and scales crop width/separation directly. Legacy UI above
1080 pixels forces scale `0.74`; failed player-count recognition retries the alternate legacy/full
profile. Custom game UI scale is not implemented; support requires the same config reader and crop
transform.

Daemon market service owns:

- Warframe Market item-slug mapping.
- Deduplicated concurrent fetches and bounded request queue.
- Response-header-aware rate limiting, retry/backoff, and stale-on-error behavior.
- Per-item price cache with fetched timestamp and explicit aggregation policy.
- One typed result contract used by overlay, desktop UI, CLI, and query dataset.

Do not copy AlecaFrame's private `api.alecaframe.com/prices/priceData` endpoint. Query
Warframe Market public data directly and document how displayed price is computed. A good
initial source is v2 `/orders/item/{slug}/top`, which returns up to five online sell and
five online buy orders in marketplace ranking order. Expose lowest sell and highest buy as
separate values; also retain orders, sample count, platform/crossplay context, API version,
and fetched time so UI does not present either as an authoritative fixed price. Current
public limit is three requests per second and no documented batch quote endpoint exists,
so requests must pass through one daemon queue with coalescing and caching. Do not derive
AlecaFrame's historical `volume` field until a supported public source is documented.

Inventory, mastery, relic planning, order management, and history belong in graphical clients
using the same daemon services. They must not be embedded in overlay rendering code.

## UI State In Memory

Overwolf's published Warframe provider exposes inventory, highlighted items, username, and chat.
That is its API boundary, not proof that other game state is unavailable. AlecaFrame operates above
that boundary, so its era OCR also does not rule out a direct collector.

The current `Warframe.x64.exe` contains useful semantic anchors:

- `p.eraTag` and `mVoidProjections`;
- `OpenVoidProjectionSelectionScreenRMI`, `VoidProjectionSelectionRMI`, and the matching close RMI;
- `VoidT1` through `VoidT5`.

A bounded Ghidra reference pass found no direct code references to those strings. They appear to be
registration or reflection data, so they establish likely state names but not a pointer path. The
selected era may live in the local `ThemedProjectionManager` movie model, while the game-rules RMI
state may identify only that the picker is open.

Live inspection on the Neo relic screen confirmed that `p.eraTag` belongs to a packed metadata-name
table, not an active-state field. `VoidT1` through `VoidT5` also occur in the intern table and
retained worldstate JSON. Neither is a usable state anchor without a traced owner.

Any collector should:

- resolve from executable signatures or registered objects, never fixed addresses;
- tolerate ASLR and fail closed when a game update changes layout;
- validate context and enum range before publishing state;
- be tested across picker changes, process restarts, and at least one game update.

Next validation is a live differential scan while cycling Lith, Meso, Neo, Axi, and All. It should
first determine whether one stable value changes with the selected tab, then trace that value back
to a semantic owner. Raw matches for localized era text are insufficient because UI strings may
remain cached after the tab changes.

## Safety Boundary

Digital Extremes states that all third-party software, including Overwolf apps, is used at
the player's own risk. No external integration can be called ban-safe.

Allowed project techniques:

- `/proc`, Steam, Proton, cgroup, and KWin metadata for process/window identity.
- Receiving `OutputDebugString` through Wine's DBWIN mapping/events without debugger attach.
- External read-only Warframe memory observation with bounded payloads, versioned signatures,
  validation, and no write-capable process access.
- Public HTTP APIs and exported game/catalog data.
- Wayland layer-shell rendering outside the game process.
- User-authorized portal capture and local OCR.

Prohibited unless the project makes a new explicit risk decision:

- Process-memory writes, debugger control, injected Wine DLLs, Vulkan implicit layers,
  DXVK/vkd3d hooks, or render interception.
- Modifying game files, intercepting network traffic, or bypassing anti-cheat.
- Synthetic input, gameplay automation, or actions not directly initiated by the user.
- Uploading screenshots, logs, inventory, or account data without explicit informed opt-in.

This deliberately does not reproduce Overwolf's injection mechanism. Wayland layer shell
meets game-only, fullscreen, and performance requirements without entering Warframe's
process or graphics stack.

DBWIN events identify relic-selection and reward-screen lifecycle without OCR. Current visual
labels still use local capture and OCR until a read-only memory collector meets the validation
requirements above.

## Local Evidence Map

- `package/manifest.json`: Warframe targeting plus desktop and `in_game_only` windows.
- `package/web/assets/js/background.js`: screenshot request, overlay window lifecycle,
  and GEP inventory forwarding.
- `csharp/.../Utils/EELogProcessor.cs`: reward/log triggers.
- `csharp/.../OCRHelper.cs`: DBWIN listener, screenshot crops, OCR, item matching, and price enrichment.
- `csharp/.../StaticData.cs`: AlecaFrame API host construction, including `/ml`.
- `csharp/.../Data/PriceHelper.cs`: batched lookup and 15-minute cache.
- `csharp/.../OverwolfWrapper.cs`: Warframe Market v2 order access and account features.
- `research/overwolf/extracted/game-events-provider/plugins/64/gep_warframeext.dll`: native
  Warframe GEP collector.
- `research/overwolf/decompiled/gep_warframeext.c`: Ghidra pseudocode showing read-only process
  access, inventory JSON extraction, and highlighted-item extraction.

## External References

- [Overwolf Electron overlay API](https://dev.overwolf.com/ow-electron/reference/Overwolf-electron-APIs/overlay/interfaces/IOverwolfOverlayApi/)
- [Overwolf overlay diagnostics](https://dev.overwolf.com/ow-native/guides/test-your-app/ow-logs/overlay-game-html/)
- [Overwolf Warframe game events](https://dev.overwolf.com/ow-native/live-game-data-gep/supported-games/warframe/)
- [Digital Extremes third-party software policy](https://forums.warframe.com/topic/1383123-third-party-software-usage/)
- [Wine `OutputDebugString` implementation](https://gitlab.winehq.org/wine/wine/-/blob/master/dlls/kernelbase/debug.c)
- [Valve Proton source and runtime documentation](https://github.com/ValveSoftware/Proton)
- [KDE LayerShellQt API](https://api.kde.org/legacy/plasma/layer-shell-qt/html/window_8h_source.html)
- [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/)
- [XDG ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html)
- [XDG Global Shortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- [Warframe Market API overview](https://docs.warframe.market/docs/api/overview/)
- [Warframe Market order endpoints](https://docs.warframe.market/docs/api/orders/)
- [Warframe Market integration rules](https://docs.warframe.market/docs/rules/overview/)
