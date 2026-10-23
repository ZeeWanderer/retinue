# Retinue

Linux/Proton game companions with a shared daemon, CLI, and MCP server.

## Warframe

Relic rewards, relic recommendations and ducat-kiosk prices in-game. Foundry,
Mastery Helper, Inventory, Market and build planning in a native Qt desktop app.

![Relic rewards with prices and collection progress](docs/images/wfcompanion-relic-rewards.webp)

[Companion features and setup](docs/companion.md) | [Desktop GUI](docs/gui.md)

## Path of Exile 2

Hover an item and press **Alt+E** to check prices, edit trade filters or open the
trade website. Optional estimates help gauge equipment value.

![PoE2 Evaluate overlay with item filters and an estimated value](docs/images/poe2companion-evaluate.webp)

[PoE2 features and setup](docs/poe2.md)

## Applications

| Application | Purpose |
| --- | --- |
| [`retinue`](docs/cli.md) | Game-scoped terminal commands, daemon controls, and stdio MCP entrypoint |
| `wfcli`, `poe2cli`, `arccli` | Game-scoped aliases for `retinue` |
| [`retinue-daemon`](docs/daemon.md) | Shared service host with separate Warframe and PoE2 supervisors |
| [`wfcompanion`](docs/companion.md) | Warframe observer and Wayland overlay |
| [`poe2companion`](docs/poe2.md) | Path of Exile 2 observer and Evaluate price-check overlay |
| [`arccompanion`](docs/arc.md) | ARC Raiders launch scaffold; observation not implemented |
| [`wfgui`](docs/gui.md) | Native Qt desktop client for Warframe account and world-state views |
| [`retinue-inspect`](docs/wfinspect.md) | Generic memory/capture inspection and Warframe research; `wfinspect` is an alias |

Clients share daemon caches and rate-limited requests. ARC Raiders is a launch
scaffold only.

## Install

Install with [Homebrew](https://brew.sh) on Linux:

```bash
brew tap zeewanderer/sundries
brew install zeewanderer/sundries/retinue
```

Upgrading from `wfcli`? Follow [storage migration](docs/data-sources.md#migrate-wfcli-storage)
to retain settings and saved builds. Source builds are covered [below](#build).

### Steam launch mode

Find the installed companion:

```bash
command -v wfcompanion
```

Set **Warframe > Properties > General > Launch Options** in Steam using that absolute path:

```text
/absolute/path/to/wfcompanion launch -- %command%
```

Start `retinue-daemon` from the host before launching Warframe; Proton-side startup is not recommended:

```bash
retinue daemon ensure
```

Launch mode starts the companion with Warframe and reuses `retinue-daemon`. See the
[companion guide](docs/companion.md) for automatic Steam configuration and overlay controls.
PoE2 has its own [companion setup](docs/poe2.md).

### Desktop launcher

Register the GUI in the desktop application menu and install its notification identity:

```bash
wfcli gui install
```

### Daemon service (optional)

Normal CLI, GUI, and companion use starts `retinue-daemon` automatically. Enable login autostart only
when persistent watches or notifications should remain active without an open client:

```bash
retinue daemon autostart enable
```

Homebrew installs use `brew services`; source builds use a systemd user unit.
Inspect either with `retinue daemon autostart status`.

## Build

Install the [build dependencies](docs/developer/workflows.md#build), then:

```bash
git submodule update --init
export VCPKG_ROOT=/path/to/vcpkg
make build
```

`make dev` and `make prod` build one flavor. Executables are in `dev/bin/` and
`prod/bin/`; root links with a `d` suffix select development builds.
See [development workflows](docs/developer/workflows.md) for individual targets and tests.

## Run

Use installed commands, or the production links after `make prod`:

```bash
retinue doctor
retinue warframe fissures
retinue warframe baro inventory
wfcli query 'dataset=worldstate type=fissure hard=true'
retinue poe2 companion status
poe2cli companion events --json
wfgui
```

Normal data commands start or reuse `retinue-daemon`; `doctor` only checks existing
state. Built-in help covers the complete command surface:

```bash
retinue --help
retinue warframe --help
retinue poe2 --help
retinue COMMAND --help
```

Register the MCP adapter with a host using the same production client:

```bash
codex mcp add retinue -- retinue mcp
```

## Documentation

| Guide | Contents |
| --- | --- |
| [Command-line client](docs/cli.md) | Command groups, help, completion, paths, and notifications |
| [Query language and watches](docs/query.md) | Datasets, expressions, raw paths, sorting, paging, and subscriptions |
| [Data sources and updates](docs/data-sources.md) | Provenance, managed caches, and refresh behavior |
| [Forma planner](docs/forma-plan.md) | Configuration, constraints, output, and visualization |
| [Daemon control](docs/daemon.md) | Lifecycle, idle policy, autostart, compatibility, and hot updates |
| [Linux/Proton companion](docs/companion.md) | Steam setup, overlay controls, diagnostics, previews, and player data |
| [Path of Exile 2](docs/poe2.md) | Price checking, native observation, launch mode, and diagnostics |
| [Desktop GUI](docs/gui.md) | Pages, loading behavior, notifications, and screenshots |
| [MCP server](docs/mcp.md) | Registration, tools, resources, and cancellation |
| [Contributor documentation](docs/DEVELOPER.md) | Architecture, source ownership, tests, and development workflows |

## License

Project code is licensed under Apache-2.0; see [LICENSE.md](LICENSE.md). Third-party components
and assets retain their own terms. Warframe assets are used under the
[Warframe Content Policy](https://www.warframe.com/en/contentpolicy); provenance notes live beside
the relevant files.
