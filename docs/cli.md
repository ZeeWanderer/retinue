# Command-Line Client

`wfcli` parses commands, submits typed requests to `wfdaemon`, and formats replies. Fetching,
persistence, query execution, and planning remain daemon-owned.

## Commands

Focused commands provide common Warframe views without query syntax:

```bash
wfcli fissures
wfcli alerts --watch
wfcli sorties
wfcli baro inventory
wfcli teshin
wfcli archimedea
wfcli circuit
wfcli circuit steel-path
```

`circuit` shows this week's Normal Warframe and Steel Path Incarnon Genesis choices.
Use `normal` or `steel-path` to select one track. `endless-xp` remains an alias.

Catalog and account commands search one domain with domain-specific output:

```bash
wfcli mods toxin
wfcli items braton
wfcli codex 'critical chance'
wfcli enemies corrupted
wfcli drops serration
wfcli market 'saryn prime set'
wfcli player
wfcli diagnostics unresolved
wfcli diagnostics logs
wfcli diagnostics logs companion --lines 20 --json
```

`diagnostics logs` reads recent daemon and companion incidents, including when daemon is stopped.
Select `daemon` or `companion` to narrow the output. [Companion diagnostics](companion.md#diagnostics)
cover collector health and capture status.

Advanced operations have dedicated guides:

- [`query` and `watch`](query.md): cross-dataset expressions and subscriptions
- [`forma-plan` and `visualize`](forma-plan.md): polarity planning
- [`build-stats`](build-stats.md): experimental direct-fire estimates
- [`daemon`](daemon.md): service lifecycle and updates
- [`companion`](companion.md): game observer and overlay control
- [`mcp`](mcp.md): structured stdio integration

Run `wfcli help commands` for every focused worldstate command.
`wfcli COMMAND --help` is the complete option reference.

## Help

Help is contextual; these forms are equivalent:

```bash
wfcli help daemon autostart
wfcli daemon autostart help
wfcli daemon autostart --help
wfcli daemon autostart -h
```

For query commands, arguments after `--` are literal text. Correction prompts require an
interactive terminal; `--no-suggest-prompt` disables them.
Invalid arguments return exit status 2; failed operations return 1. Diagnostics use stderr.

Commands narrow an operation. Options modify it. Where both forms improve shell use, both are
accepted:

```bash
wfcli baro inventory
wfcli baro --inventory
wfcli archimedea deep
wfcli archimedea --deep
```

Archimedea predictions use cached player inventory and ordered game equipment pools.
After game updates, the last captured pools remain usable with an unverified-loadout warning.

## Output

`--json` and `--utc` work before or after any command. Human timestamps use local time
with a UTC offset; `--utc` selects RFC 3339 UTC. JSON uses UTC and preserves raw epoch
fields in their original units. Watches emit one JSON object per update.

```bash
wfcli daemon status --json
wfcli --utc companion status
wfcli query --json 'dataset=incidents timestamp>=now-1h'
```

`--format table|block|json` selects presentation on query commands. The last format
option wins. See [query syntax](query.md) for filtering and time comparisons.

## Bash Completion

Staged and packaged builds include lazy bash-completion files. Install a user copy when running
the escript directly or overriding a packaged completion:

```bash
wfcli completion install
wfcli completion status
wfcli completion uninstall
```

Use `--dir PATH` for another completion directory. Bash 5.3 or newer and `bash-completion` are
required. Rerun `install` after updating a direct escript. For one shell:

```bash
source <(wfcli completion bash)
```

The generated function contains all candidates. Shell startup and Tab completion do not start an
Erlang VM.

## Application Paths

```bash
wfcli paths
wfcli paths --apps
wfcli paths wfdaemon
wfcli paths wfgui
wfcli companion paths
```

The default report merges all managed XDG directories into one filesystem tree without creating
them. A symlink is rendered as `name -> target`, with logical descendants nested below it. Use
`--apps` to group paths by owning application, or name one application to inspect it alone.

## Fissure Notifications

```bash
wfcli notifications status
wfcli notifications off
wfcli notifications on
wfcli notifications persistent
```

`on` watches while at least one `wfgui` connection is open. `persistent` watches whenever
`wfdaemon` is running. The setting is shared with the desktop GUI and persists across restarts.

See [Data sources and updates](data-sources.md) for cache refresh commands.
