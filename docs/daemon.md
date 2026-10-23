# Daemon Control

`retinue-daemon` hosts shared lifecycle and transport with separate game services. Warframe owns
fetching, queries, watches, Market access, player state, and Forma planning; PoE2 owns normalized
log-session evidence. Normal client commands start the daemon automatically.

## Lifecycle

```bash
retinue daemon status
retinue daemon ensure
retinue daemon start
retinue daemon start --idle-shutdown
retinue daemon start --idle-timeout 1800
retinue daemon restart
retinue daemon stop
```

`status` reports installation source and root, build identity, both client protocol contracts,
runtime queues, and idle policy.

Implicit startup stops the daemon after ten idle minutes. Explicit `start` and `restart` keep it
running until `stop` unless an idle option is supplied. Starting an already-running daemon applies
the requested idle policy. `ensure` starts an absent daemon without changing a running daemon.

Queued work, subscriptions, and active companion connections prevent idle shutdown.

## Login Autostart

```bash
retinue daemon autostart status
retinue daemon autostart enable
retinue daemon autostart disable
```

`enable` installs and starts a systemd user service for a source build. `disable` prevents login
startup without stopping the running daemon.

Development and production use separate units:

- `retinue-daemon-dev.service` for development builds
- `retinue-daemon.service` for production builds

The client detects its installation and refreshes stale service ownership before startup.
Source-build services wait up to 90 seconds for the executable and state directory
when a drive mounts late at login. During this wait the service is `activating`.

## Updates

Each connection checks required interface versions, build flavor, and a fingerprint of
`retinue_core`, `wfcore`, and `retinue_daemon`. Stale modules are hot-loaded automatically;
unresolved mismatches restart the requested release.

A running daemon also watches the staged `BUILD_ID`, so replacing a repository or packaged build
does not require a client request. Manual update remains available:

```bash
retinue daemon update
retinue daemon update --release RELEASE
```

The first form loads changed BEAMs from the active staged build. The second applies a versioned OTP
release upgrade package.

## Shared Work

One-off requests are monitored until one reply is delivered. Watches remain subscribed. If a
client exits, its queued or active operation is cancelled without polling.

Worldstate watches share one fetch and parse cycle. Market requests use a separate rate-limited
queue and coalesce matching work. Managed knowledge refreshes through its own serialized source
queue; see [Data sources and updates](data-sources.md).

Market account mutations are serialized separately from public quotes. The session token and
presence mode are owner-only state files; passwords are neither persisted nor returned to clients.
An online Market presence prevents idle shutdown.

Fissure notification policy is persisted by the daemon. Session mode runs while a GUI is
connected; persistent mode runs for the daemon lifetime. A new watch records its first snapshot,
then notifies about matching fissures added later.

## Logs

Warnings and errors are saved to `$XDG_STATE_HOME/retinue/daemon.log`, normally
`~/.local/state/retinue/daemon.log`. The log rotates at 1 MiB and keeps two archives.
Knowledge refresh incidents include the source, failing URL and reason, including mirror failures
that recover through a fallback.

Crash dumps are written to `$XDG_STATE_HOME/retinue/erl_crash.dump`, normally
`~/.local/state/retinue/erl_crash.dump`. Use `retinue daemon paths` for all daemon directories.

Native clients share `$XDG_RUNTIME_DIR/retinue/daemon.sock`; without a valid runtime base,
the owner-only fallback is `/tmp/retinue-UID/daemon.sock`. `RETINUE_DAEMON_SOCKET` overrides
the socket path. Game caches and durable data live below `retinue/games/<game>` in their
respective XDG roots; see [storage](data-sources.md#storage).

Implementation details are in [Daemon architecture](developer/daemon.md).
