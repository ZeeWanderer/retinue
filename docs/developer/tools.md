# Helper Scripts

These scripts are for quick local inspection of cached data. They do not change files.
Defaults use `$XDG_CACHE_HOME/retinue/games/warframe` (`~/.cache` when unset).

## Worldstate inspection

- List top-level keys and known sections:

```bash
scripts/worldstate_inspect.py --top-keys
scripts/worldstate_inspect.py --syndicate-tags
scripts/worldstate_inspect.py --calendar
```

- Search keys or values with regex:

```bash
scripts/worldstate_inspect.py --keys "Calendar" --values "Calendar1999"
```

You can also pass a specific cache path:

```bash
scripts/worldstate_inspect.py ~/.cache/retinue/games/warframe/worldstate.json --top-keys
```

## Language lookup

- Lookup a language map key:

```bash
scripts/lang_lookup.py "/Lotus/Types/Challenges/Calendar1999/CalendarKillTechrotEnemiesEasy"
```

## Export file utilities

- Search exports quickly:

```bash
scripts/priv_search.sh "Calendar1999"
```

- List common keys in an export file:

```bash
scripts/export_keys.py ExportUpgrades_en.json uniqueName name locName
```

## Forma-plan benchmarking

```bash
scripts/bench_forma_plan.escript docs/forma_plan.example.yml
```
