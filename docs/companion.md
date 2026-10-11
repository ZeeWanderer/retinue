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
wfcli companion diagnostics status
wfcli companion diagnostics watch inventory --seconds 60
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

## Diagnostics

```bash
wfcli companion status
wfcli companion probe
wfcli companion logs
wfcli companion paths
wfcli companion screenshot ./capture.png
wfcli companion capture arm relic-reward
wfcli companion capture status
wfcli companion capture cancel
wfcli companion relic-ocr ./capture.png
wfcli companion relic-ocr
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
wfcli companion preview list
wfcli companion preview image relic-rewards
wfcli companion preview video relic-suggestions
make previews
```

`make previews` renders registered scenes at 1920x1080 and 2560x1440. Matching fixtures provide
backgrounds; other outputs are transparent. Filter output with Make variables:

```bash
make previews PREVIEW_MEDIA=image
make previews PREVIEW_SCENES='relic-rewards relic-suggestions'
make previews PREVIEW_RESOLUTIONS=2560x1440
```

Direct previews can use real OCR and daemon data:

```bash
wfcompanion preview image relic-rewards --scan SCREENSHOT --background SCREENSHOT OUTPUT.png
wfcompanion preview image relic-suggestions --era all --background SCREENSHOT OUTPUT.png
```

Animated previews require FFmpeg. AlecaFrame reference previews require
[`make aleca-layout-setup`](../tools/aleca-layout/README.md).

## Player Dataset

```bash
wfcli player
wfcli player source=game
wfcli query 'dataset=player source=inventory'
```

The dataset contains game process state, collector health, and the latest account inventory and
mastery observation. It is stored in the daemon's owner-only local cache. Implementation and
protocol details are in [Companion architecture](developer/companion.md).
