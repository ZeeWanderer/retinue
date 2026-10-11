# Path of Exile 2

`poe2companion` observes local `Client.txt` logs and publishes area, character-level, and death
events to Retinue's PoE2 service.

## Observe

Build the [source installation](../README.md#build), then run:

```bash
retinue daemon ensure
poe2companion watch
```

`watch` discovers Steam app `2694490` and reads its `logs/Client.txt`. To select a log explicitly:

```bash
poe2companion watch --log /absolute/path/to/logs/Client.txt
```

The default command is `watch`. Observation starts at the current end of the file, so existing
lines are not presented as fresh gameplay. With `--log`, the parent directory must already exist;
the file may appear later. The companion observes appended lines and reconnects to the shared
daemon as needed.

## Steam Launch

Use the absolute path to the built companion in **Path of Exile 2 > Properties > General >
Launch Options**:

```text
/absolute/path/to/poe2companion launch -- %command%
```

An explicit log path can precede `--`:

```text
/absolute/path/to/poe2companion launch --log "/absolute/path/to/logs/Client.txt" -- %command%
```

Launch mode passes the child command and arguments unchanged and preserves its normal exit
status. Observer failure does not terminate the game.

## Status And Events

```bash
retinue poe2 companion status
retinue poe2 companion events
poe2cli companion status
poe2cli companion events --json
```

`poe2cli` is the game-scoped alias for `retinue poe2`. Status shows connected log observers,
source, revision, and last-observed zone and actors. An observer connection can outlive the
game. Events show the recent log history. Add `--json` for the full structured response.

Supported log evidence:

- Generated area ID, area level, and seed
- Displayed scene/area name
- Named character level and class
- Named character death

Area level and character level are separate fields. Actors retain the names observed in the
log. Game timestamps remain source-local; `--utc` applies to the daemon's receipt timestamp.

## Offline Inspection

```bash
poe2companion inspect --log /absolute/path/to/logs/Client.txt
```

`inspect` requires `--log` and prints historical domain records as JSON lines, entirely offline.

## Storage

```bash
poe2companion paths --json
retinue paths --apps
```

The native path report shows the shared daemon socket's parent directory without creating it.
The daemon owns the session, cursor, and at most 128 cached events in
`$XDG_CACHE_HOME/retinue/games/poe2/session.json`, listed under `retinue_daemon` in the aggregate report.
