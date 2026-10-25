# Data Sources and Updates

## Sources

| Data | Source | Managed by |
| --- | --- | --- |
| Missions, cycles, alerts, fissures, and other live state | Official Warframe worldstate | Worldstate cache and watch service |
| Mods, items, recipes, names, and Codex records | Official PublicExport files | Managed export catalog |
| Enemies, drops, relic rewards, components, and image identities | WFCD `warframe-items` | Managed WFCD catalog |
| Star Chart mastery values | `warframe-public-export-plus` regions | Managed mastery catalog |
| Public listings and top orders | Warframe Market | Market cache and request queue |
| Inventory, mastery, and local game state | `wfcompanion` observation | Owner-only player store |

Parsers attach typed fields and resolved names without replacing source records. Unified queries
can use normalized fields or `data.<path>` values; see [Query language and watches](query.md).

## Manual Updates

```bash
wfcli update
wfcli update --default
wfcli update --all
```

No flags and `--default` refresh Sol nodes, languages, PublicExport files, WFCD data, and Star
Chart mastery values. `--all` selects every managed source.

Targeted metadata options:

- `--nodes`, `--languages`, `--manifest`, `--exports`
- `--recipes`, `--upgrades`, `--weapons`, `--warframes`, `--resources`
- `--wfcd`

Live-cache options are `--worldstate` and `--trader`. Run `wfcli update --help` for path
overrides and the complete option list.

## Automatic Refresh

`retinue-daemon` checks managed metadata hourly and refreshes sources older than 24 hours. Missing or
invalid managed files are fetched when first needed. Refreshes share a serialized source queue and
publish completed files by atomic rename, so active readers retain a complete old version until a
new version is ready.

Worldstate, Market manifests and quotes, relic tables, and image assets use request-specific cache
policies. Failed refreshes retain the last valid cached value where available.

Explicit `--knowledge-dir` and `--exports-dir` paths are caller-owned: the daemon validates them but
does not modify them.

## Storage

Run `retinue paths` to inspect managed directories. Platform roots use `retinue`; game-owned
files use `retinue/games/<game>` beneath the appropriate XDG root.

```text
~/.config/retinue/games/warframe
~/.local/share/retinue/games/warframe
~/.cache/retinue/games/warframe
~/.local/state/retinue/games/warframe
$XDG_RUNTIME_DIR/retinue/daemon.sock
```

Managed WFCD files record source URL, fetch time, and content SHA-256. Downloaded assets are
content-addressed so identical images share one cached object.

Saved build groups are durable data in `builds.term` beneath the Warframe data root.
Refetchable catalogs, original asset objects, observations, and plan results stay in cache.
GUI derivatives use the Warframe cache's `gui/derivatives/v1` (Qt) and
`gui/derivatives/native-v1` (native renderer) directories. Daemon incidents and
crash dumps belong to the platform state root; companion incidents remain game-owned.
PoE2 has a separate [log-session cache](poe2.md#storage).

## Migrate wfcli Storage

Before upgrading, stop a legacy Homebrew service with `brew services stop wfcli`.
For a source installation, disable its old unit with
`systemctl --user disable --now wfdaemon.service`.

Stop both daemons, the GUI, and companions before copying. With Homebrew, review the
migration plan, then apply it:

```bash
"$(brew --prefix retinue)/share/retinue/migrate-wfcli-storage" --dry-run
"$(brew --prefix retinue)/share/retinue/migrate-wfcli-storage" --apply
```

From a source checkout:

```bash
./scripts/migrate-wfcli-storage --dry-run
./scripts/migrate-wfcli-storage --apply
```

The default is `--dry-run`. The script uses your current XDG roots to copy legacy `wfcli`
stores into Retinue, placing build groups in durable data, renaming GUI config/cache paths,
and placing daemon logs in platform state. It retains the originals and aborts before copying
if any destination exists or two sources target the same destination. Resolve reported
conflicts before retrying; existing Retinue data is never merged or overwritten.
After migration, run `wfcli gui install` to refresh the desktop launcher. Re-enable login
autostart with `retinue daemon autostart enable` if needed.
