# Installation

## Homebrew

```bash
brew tap zeewanderer/sundries
brew install zeewanderer/sundries/retinue
```

The overlays currently require KDE Plasma on Wayland. Warframe also needs KScreen
and Tesseract 5 with English language data. CLI and research tools do not require
a running game or desktop session unless their command uses one.

Upgrading from `wfcli`? [Migrate storage](data-sources.md#migrate-wfcli-storage)
to retain settings and saved builds. For source builds, follow
[development workflows](developer/workflows.md#build).

## Steam

Find the installed companion with `command -v wfcompanion` or
`command -v poe2companion`. Put its absolute path in the game's
**Properties > General > Launch Options**:

```text
/absolute/path/to/wfcompanion launch -- %command%
```

Use `poe2companion` for Path of Exile 2. Existing wrappers such as `gamemoderun`
go after `--`. See the [Warframe](companion.md#steam-launch-mode) and
[PoE2](poe2.md#steam-launch) guides for controls and game-specific setup.

Start the daemon on the host before launching through Proton:

```bash
retinue daemon ensure
```

## Desktop App

```bash
wfcli gui install
wfgui
```

This registers the Warframe GUI in the application menu and installs its
notification identity. See the [GUI guide](gui.md).

## Optional Service

Clients normally start the daemon when needed. Enable login autostart for
persistent watches or notifications without an open client:

```bash
retinue daemon autostart enable
retinue daemon autostart status
```

Homebrew installations use `brew services`; source builds use a systemd user unit.
See [daemon control](daemon.md) for lifecycle and idle behavior.

## Check The Setup

```bash
retinue doctor
retinue warframe --help
retinue poe2 --help
```

`doctor` inspects existing state without starting the daemon. A memory adapter
that has not run remains unverified until the corresponding game state is seen.
See [CLI usage](cli.md), [diagnostics](daemon.md) and [MCP setup](mcp.md).
