# Export Files

Official PublicExport files provide item names, metadata, recipes, and identifier lookups.

## Storage And Updates

Managed files live in the per-user XDG cache. Bundled fallback data under
`apps/retinue_daemon/priv/` supports offline resolution and tests. `wf_source_manager` validates
required files and `wf_metadata_update` downloads replacements atomically.

Explicit `--exports-dir` paths are caller-owned. Missing or invalid files there fail with
`source_unavailable`; the daemon never modifies the directory.

Common files include:

- `ExportUpgrades_en.json`
- `ExportWeapons_en.json`
- `ExportWarframes_en.json`
- `ExportResources_en.json`
- `ExportRecipes_en.json`
- `ExportRelicArcane_en.json`
- `ExportManifest.json`

The complete managed list is `?EXPORT_FILES` in
`apps/retinue_daemon/src/games/warframe/catalog/wf_metadata_update.erl`.

## File Scopes

PublicExport has three deliberate scopes:

- `export_files/0`: every managed official file
- `resolver_export_files/0`: files scanned for identifier resolution
- `codex_export_files/0`: files contributing searchable Codex records

Keep these scopes separate. Large Codex files do not belong on the hot identifier-resolution path.

## Query Path

`wf_exports_store` owns source preparation, signatures, entity caches, and request execution.
`wf_entity_exports` builds typed query entities. The focused `mods` and `items` commands and
unified `query` submit to that service; CLI modules only parse flags and render replies.

`mods` searches upgrades by type, polarity, rarity, compatibility, drain, and text. `items`
searches names and fields across selected exports. An explicit `file=` selector may load a managed
file outside the default item set. Query syntax is documented in
[Query language and watches](../query.md).

## Inspection

Use `rg` directly or the repository helpers:

```bash
rg -n "Calendar1999" apps/retinue_daemon/priv
scripts/priv_search.sh "Calendar1999"
scripts/export_keys.py ExportUpgrades_en.json uniqueName name locName
```

See [Helper scripts](tools.md) for other data-inspection commands.
