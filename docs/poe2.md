# Path of Exile 2

`poe2companion` reads the running game's memory and publishes character, XP, gold,
life, area and panel state to Retinue's daemon. Observation runs once per second;
changed snapshots are published immediately.

## Steam Launch

Build the [source installation](../README.md#build), then set **Path of Exile 2 >
Properties > General > Launch Options**:

```text
/absolute/path/to/poe2companion launch -- %command%
```

The launched command, arguments and exit status are preserved. Observer failure
does not terminate the game. For development, use `poe2companiond`: rebuilding with
`make dev-companion` reloads it without restarting the game. Set
`POE2COMPANION_DEV_RELOAD=0` to disable reload.

To attach separately to an already running game:

```bash
retinue daemon ensure
poe2companion watch
```

`watch` is the default command. Use `--pid PID` to select a particular process.

## Evaluate

On KDE Wayland, hover an item and press **Alt+E**. Companion copies its item text,
opens the in-game **Evaluate** panel, and resolves searchable attributes using current trade
metadata. It retains your selected league or defaults to the trade API's first
league. Check the league in the title bar, click modifiers to include them,
adjust their ranges, then select **Search** or press Enter outside a numeric field.
Crafting bases, identified uniques and categories outside equipment, jewels and
maps search automatically when trade rate usage is below 75%. Other items wait
for Search. Character-league detection is not yet available.

Alt+E is bound to the focused game window, not registered as a global desktop
shortcut. KDE may separately ask permission to send Ctrl+C for item copying.

The panel shows up to ten asking-price listings. The scales icon opens the full
search on the trade website. Search defaults to **Instant Buyout**, matching the
reference provider. Settings and result selectors offer status, currency and
listing-age filters. Unresolved modifiers remain readable without range controls;
hover for the reason. Equivalent trade IDs are searched as alternatives, and
repeated stats are combined. English equipment, skill gems, currency and quest
item text is supported. Items absent from the current trade catalog remain
readable but cannot be searched.

Supported equipment also requests an **Estimated Value** automatically, independently
of Search. It shows Exalted Orbs, a price range and reliability when available.
This sends normalized item properties and modifiers to `prediction.poeoverlay.com`,
without account details, inventory location, credentials or raw clipboard text.
Estimates use the whole item, not selected search filters. Editing filters does
not re-estimate. Results are cached for ten minutes; unavailable models or
unsupported items leave the plaque hidden. Search still works. Estimates are
not guaranteed sale prices.

Gem level, quality and socket filters are selected initially. Click an attribute's
text to include or exclude it; excluded numeric fields are disabled. Excluding a
filter preserves its edited bounds. Rarity and item-level controls sit beside
the item; condition labels such as **Modifiable** toggle their search filters.
**Quick Price** defaults for ordinary rare items and selects up to four ranked
properties or modifier groups. Weapon DPS, attack speed, critical chance and
defence properties can be used as filters. Local modifier scores transfer to
their properties when the catalog resolves their internal identities; the source
modifiers are then excluded from profile selection. Modifiable weapon/defence
values use quality normalization, matching the reference provider.
**Crafting Base** matches the base, item level and fixed modifiers;
**Exact Match** defaults for uniques and unmodifiable items. **Broad (-10%)** keeps
the selection and relaxes variable bounds. Switching profiles does not submit a
search or save a category preference. Broader pseudo-stat merging remains
unsupported; unresolved modifiers are never silently included. Incomplete catalog
joins can leave a property unranked; it can still be selected manually.

Escape closes an open menu first, then the panel. The close button closes the
panel directly. Moving outside returns pointer and
keyboard input to the game without closing the panel or requiring a click.
Alt+Tab preserves the evaluation; returning restores it without grabbing keyboard
focus. Numeric fields support caret navigation, drag/Shift selection and
Ctrl+A/C/X/V. Focus selects the number; clicking again places the caret.
Tab/Shift+Tab moves between included bounds without selecting other filters.
Drag the title or empty title-bar space
to move it; only the league name opens the selector. Zoom is in **Settings**, in
5% steps. League, search status and scale persist in
`$XDG_CONFIG_HOME/retinue/games/poe2/companion/evaluate.json`. Reload waits until
the panel closes. Use `--no-overlay` for observation without desktop integration.

For a deterministic cropped preview, without the game or a desktop session:

```bash
poe2companion preview --output /tmp/evaluate.png --scale 1.25
```

`--input FILE` accepts an evaluation/results JSON fixture; `--width` and `--height`
set the simulated output size. An optional `editor` object with `modifier` index,
`bound` (`minimum` or `maximum`) and `select_all` previews an active field.
An optional `menu` (`settings`, `rarity`, `status`, `currency`, `indexed`) opens
that menu. This uses the live overlay's renderer.

`--full-frame` keeps the requested canvas size and writes a JSON geometry sidecar.
`--state` selects `ready`, `searching`, `results` (default), `empty`, or `search-error`.
`make previews-poe2 PREVIEW_MEDIA=image` renders native and reference states at
1080p and 1440p under `previews/poe2/<resolution>/`.

## Status And Events

```bash
retinue poe2 companion status
poe2cli doctor
poe2cli diagnostics events
poe2cli diagnostics status --json
```

`poe2cli` is the game-scoped alias for `retinue poe2`. Status shows the observer,
process, current snapshot and read failures. Events track state, character, XP,
gold, area, panel changes and observed deaths. Missing or invalid reads remain
unknown. The daemon retains the snapshot and at most 128 events in
`$XDG_CACHE_HOME/retinue/games/poe2/session.json`; stored data alone does not mean
the game is still running.

## Inspection

```bash
retinue-inspect poe2 adapter
retinue-inspect poe2 snapshot --output research/poe2-capture
retinue-inspect poe2 replay research/poe2-capture
```

`adapter` discovers bindings in the installed Steam executable, or one supplied
with `--exe PATH`. `snapshot` uses the same reader as companion; `--output` saves
the reads for offline replay in a new private directory. See the
[inspector guide](wfinspect.md) for generic memory queries.

`poe2companion paths --json` reports its used directories; `retinue paths` includes
the daemon-owned session cache.
