# Worldstate Pipeline

This describes the fetch/cache/index/format flow and where to hook in.

## Data flow

1. `wfcli_worldstate_service` coalesces requests and owns refresh timing.
2. `wfcli_worldstate:load/1` fetches/decodes an immutable source tree; `index_raw/2`
   builds derived domain entries without replacing source data.
3. `wfcli_entity_worldstate` and `wfcli_worldstate_projector` attach sparse fields and
   presentation-neutral row maps in the daemon.
4. The reply crosses the protocol as data. `wfcli_worldstate_cli` controls the command,
   `wfcli_worldstate_watch_cli` owns subscription lifecycle, and `wfcli_worldstate_output`
   renders tables, blocks, source metadata, and watch changes.

## Index entries

Each entry is a map with keys like:
- `type` atom (e.g., `fissure`, `event`, `calendar`).
- `id` stable identifier (string).
- `name` display name (string).
- `data` raw worldstate map for that entry.
- `haystack` a searchable string built from raw and resolved values.
- `row_map` normalized table fields for known columns.
- `extra_fields` sparse top-level fields not in `row_map` (string values only).
- `fields` flattened values used for text matching.

Unified query also receives one `raw_worldstate` root entity. It is absent from focused
commands and has no free-text haystack. Explicit `data.<path>` filters and extracts can
therefore address every source branch, including sections with no parser, without adding
duplicate matches to normal searches.

Sparse fields are optional: they are indexed into `haystack` and may be surfaced as extra columns if they appear frequently enough. This keeps core columns stable while allowing new fields to show up when present. Block output always appends all `extra_fields` for the entry.

## Translation and naming

Translation logic is centralized in daemon module `wfcli_resolve`:
- `wfcli_resolve:resolve/3` is the single domain entry point for keys such as `item`,
  `node`, `faction`, `missiontype`, `modifier`, `season`, `dt`, and `any`.
- Resolution order: language map (`languages.json`) → export item map → raw identifier.
- Mission types, sortie bosses/modifiers, and seasons have small hardcoded fallbacks for missing keys.
- Node names resolve via daemon `priv/solNodes.json`, then small fallbacks for hidden nodes
  omitted upstream, with raw fallback when genuinely unknown.
- When managed cache and bundled metadata both exist, resolver reads newer file so stale cache
  from an older installation cannot mask newer bundled identifiers.
- `--raw` disables translation and keeps raw identifiers and UTC timestamps.
- When resolution fails, the raw identifier/path is returned as-is.

## Watch queries

- Watch specs accept `<type>` or `<type>:<query>`; `--watch` on a command uses that command as the default spec.
- Watch output prints only when changes are detected; `--always` prints every tick.
- Watch honors cache TTL between ticks; `--refresh` forces refetch.
- Queries use the shared boolean AST and evaluator. `|` means alternatives inside one filter;
  complete expressions use uppercase `OR`. See `query-language.md`.
- Use `data.<path>` to filter on raw worldstate values; paths are dot-separated.
- Use `extract=data.<path>` in watch queries to print extracted values, and pair with `--diff` to show changes between ticks.
- Diff output supports styles via `--diff-style inline|list|diff|none` (default: inline). `list` uses labeled lines, `diff` is minimal.
- Diff colors: green added, red removed, yellow changed; no `+`/`-` prefixes.

## Cache locations and refresh

- Worldstate cache defaults to `$XDG_CACHE_HOME/retinue/games/warframe/worldstate.json`, or
  `~/.cache/retinue/games/warframe/worldstate.json` when XDG cache is unset.
- Node/language maps and exports update under the per-user XDG cache. Bundled fallback copies
  live under `apps/retinue_daemon/priv/`.
- Refresh metadata with `wfcli update`; see [data sources](../data-sources.md).

## Calendar data

- Source: `KnownCalendarSeasons` in worldstate.
- Flattened into per-day entries in `wfcli_worldstate:index_raw/2`.
- CLI support in `wfcli_worldstate_cli.erl` (`calendar` command, `--day`).

## Calculated Teshin inventory

Teshin's Steel Path inventory is not present in the official worldstate snapshot. It is a
daemon-backed calculated source routed through the same inventory/query/render path:

- `wfcli_teshin.erl` owns the rotation anchor, offering catalog, and time calculation.
- `wfcli_worldstate_service.erl` accepts `source => teshin` without fetching worldstate.
- `wfcli_worldstate:inventory_entries/3` exposes normalized `teshin_item` entities.
- `wfcli teshin [query]` shows the current weekly item plus evergreen offerings.
- The command returns a one-shot calculated inventory and does not register a daemon watch.

Upstream references used for the implementation:

- Parser algorithm: `WFCD/warframe-worldstate-parser` commit
  `96b681afd03c7141ed5e21b058de11149284fbb2`,
  `lib/models/SteelPathOffering.ts`.
- Catalog: `WFCD/warframe-worldstate-data` commit
  `957f213ddcf3c1ec0e8556fc323ee8cd9a646d19`, `data/steelPath.json`.
- Rotation starts at `2020-11-16T00:00:00Z`, advances every 604800 seconds, and wraps
  after eight weeks.

To refresh this implementation after upstream changes, update the ignored clones under
`research/`, compare both files above, then update `wfcli_teshin.erl` and its deterministic
rotation tests together. There is no runtime WFCD dependency.

## Adding a new worldstate type

1. Add the index/projector behavior under `apps/retinue_daemon/src/games/warframe/`.
2. Add shared result columns in `wfcli_worldstate_schema` when needed.
3. Add the terminal block layout and command wiring under `apps/retinue_cli/src/`.
4. Add tests and fixture updates in `apps/retinue_cli/test/`.
