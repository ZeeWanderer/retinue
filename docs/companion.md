# Linux/Proton Companion

`wfcompanion` observes local Warframe state, publishes player data to `retinue-daemon`, and renders
Wayland overlays for relic rewards and relic selection.

## Requirements

- KDE Plasma on Wayland with KWin screenshot support and KScreen
- Tesseract 5 with English language data
- built `wfcli`, `retinue-daemon`, and `wfcompanion` executables

Source-build requirements are listed in the [repository README](../README.md#build).

## Steam Launch Mode

Launch mode starts and stops the companion with Warframe and provides the process relationship
required by Linux read-only process access. Set **Warframe > Properties > General > Launch
Options** in Steam to:

```bash
/absolute/path/to/retinue/prod/bin/wfcompanion launch -- %command%
```

Keep existing wrappers after `--`:

```bash
/absolute/path/to/retinue/prod/bin/wfcompanion launch -- gamemoderun %command%
```

The CLI can edit the launch option after Steam is closed:

```bash
wfcli companion install --dry-run
wfcli companion install
wfcli companion uninstall
```

Use `dev/bin/wfcompanion` or `wfcompaniond` to test a development build. Its diagnostic
HUD hides automatically when Warframe starts; production starts with it hidden.
Development builds reload when rebuilt and idle; set `WFCOMPANION_DEV_RELOAD=0`
to disable this.

## Standalone Mode

Run the companion as a detached user process:

```bash
wfcli companion start
wfcli companion status
wfcli doctor
wfcli diagnostics status
wfcli diagnostics watch inventory --seconds 60
wfcli companion restart
wfcli companion stop
```

Overlay controls:

```bash
wfcli companion show
wfcli companion hide
wfcli companion hud show
wfcli companion hud hide
```

`show` and `hide` control all overlays. `hud` controls only the diagnostic panel. Overlay pixels
are gated to the recognized Warframe window.

## Interaction

Press `Ctrl+Tab` while a relic overlay is visible to toggle interaction mode. KDE may request
global-shortcut permission on first use.

Interaction mode captures pointer input over the overlay. Press `Ctrl+Tab` again, click outside,
close the scene, or leave Warframe focus to return input to the game. Passive overlays remain
click-through.

## Relic Data

Warframe debug output triggers each relic scene. Relic selection and reward names use bounded
Scaleform state first. OCR captures after the stabilization deadline only when memory extraction
fails. The daemon resolves items, player state, assets, reward tables, and batched Market prices.

Reward cards show platinum, ducats, vault and ownership state, set components, and set value.
Selection cards rank owned relics using reward tables and current Market data.

## Ducat Kiosk

Hover a prime part to show its lowest platinum sell listing beside the native
ducat value. Component tooltips also show owned, mastered and subsumed glyphs for
the completed parent item, without waiting for prices. Prices warm in the background
while the kiosk is open. Cached prices appear immediately, including older quotes,
while stale prices refresh in the background.
No listing means no platinum label; a failed request without a previous price shows
`--`. Listings are asking prices, not guaranteed proceeds.

The reader currently recognizes English kiosk titles and item names. Placement
requires the game viewport to match the overlay output; mismatched window sizes
hide the extra row. `wfcli doctor` reports kiosk, tooltip and pricing checks.

## Diagnostics

```bash
wfcli companion status
wfcli doctor
wfcli diagnostics probe
wfcli diagnostics incidents
wfcli diagnostics logs
wfcli companion paths
wfcli diagnostics screenshot ./capture.png
wfcli diagnostics capture arm relic-reward
wfcli diagnostics capture status
wfcli diagnostics capture cancel
wfcli diagnostics relic-ocr ./capture.png
wfcli diagnostics relic-ocr
```

`status` shows collector health, receipt times, capture errors and cached game metadata.
`capture status` shows the armed request, latest job ID, phase and result.
`capture cancel` cancels both the arm and running evidence jobs. Captures have a
30-second cooperative deadline, 64 GiB read budget and 256 MiB output budget;
cancelled or failed writes retain a small terminal report. Overlay dismissal
does not cancel evidence collection. Reports from disconnected companions are
historical. `logs` supports `--lines N` and `--json`;
[shared incident viewing](cli.md#commands) also includes daemon logs.

`diagnostics status` requests live counters and recent watch jobs. Watches reuse the running
collectors, sample once per second, and end after the requested duration or client disconnect.
Use `diagnostics stop JOB` to stop one from another terminal. Add `--pid PID` when multiple
companions are connected; `--json` emits one object per update.

The screenshot command captures Warframe rather than the active desktop window. Run the full
saved-image reward pipeline with:

```bash
dev/bin/wfcompanion --relic-screenshot ./capture.png
```

An armed relic-reward capture saves a screenshot, loaded Scaleform movies, typed text-object
memory, process maps, and trigger/build metadata under the user cache. The report identifies
incomplete movies and any heap-scan fallback. Supply a directory after `relic-reward` to override it.

The installation also provides explicit process diagnostics:

```bash
wfinspect game process
wfinspect game ui state
wfinspect game ui relic
wfinspect game ui movies
wfinspect game ui find 'RESOURCE DRONES'
wfinspect game ui capture /tmp/warframe-ui 'RESOURCE DRONES'
```

Incident logs use `$XDG_STATE_HOME/retinue/games/warframe/wfcompanion.log`, normally
`~/.local/state/retinue/games/warframe/wfcompanion.log`.

## Previews

```bash
wfcli diagnostics preview list
wfcli diagnostics preview image relic-rewards
wfcli diagnostics preview video relic-suggestions
make previews
```

`make previews` (also `make previews-all`) renders all supported games at 1920x1080 and 2560x1440.
Use `make previews-wf` or `make previews-poe2` to select one game.
Still previews are compact comparisons at
`previews/<game>/<resolution>/side-by-side/<scene>.png`: original left, Retinue right,
at their rendered pixel sizes. Comparisons require both providers to render the
same scene successfully. Adjacent JSON records crop bounds; source renders and
reference HTML remain under `companion/` and `reference/`, including unpaired scenes.
Use `PREVIEW_LAYOUT=screen` for full-screen placement checks with fixture
backgrounds. Videos remain full-screen. Filter output with Make variables:

```bash
make previews PREVIEW_MEDIA=image
make previews-wf PREVIEW_SCENES='relic-rewards relic-suggestions'
make previews PREVIEW_RESOLUTIONS=2560x1440
make previews PREVIEW_LAYOUT=screen PREVIEW_MEDIA=image
```

Direct previews can use real OCR and daemon data:

```bash
wfcompanion preview image relic-rewards --scan SCREENSHOT --background SCREENSHOT OUTPUT.png
wfcompanion preview image relic-suggestions --era all --background SCREENSHOT OUTPUT.png
```

Comparisons use the reference tooling's Chromium installation. Animated previews
require FFmpeg. Setup and PoE2 previews are documented
in [`tools/ui-reference`](../tools/ui-reference/README.md).

## Player Dataset

```bash
wfcli player
wfcli player source=game
wfcli query 'dataset=player source=inventory'
```

The dataset contains game process state, collector health, and the latest account inventory and
mastery observation. It is stored in the daemon's owner-only local cache. Implementation and
protocol details are in [Companion architecture](developer/companion.md).
