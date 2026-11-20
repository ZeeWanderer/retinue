# Retinue Documentation

Linux/Proton game companions, desktop tools and shared services.

## Use Retinue

[Install and connect](installation.md), then run `retinue doctor` to check your
setup. Overlays require KDE Plasma on Wayland.

| Game | Tools |
| --- | --- |
| Warframe | [In-game companion](companion.md), [desktop app](gui.md), [Forma planning](forma-plan.md) |
| Path of Exile 2 | [Evaluate items and search trade listings](poe2.md) |
| ARC Raiders | [Launch scaffolding; no gameplay integration yet](arc.md) |

- [Commands and diagnostics](cli.md): check services, companions and game data.
- [Queries and watches](query.md): select data and follow changes.
- [MCP](mcp.md): expose Retinue tools to a compatible client.
- [Game inspection](wfinspect.md): inspect memory, captures and resources.

## Build Interfaces

The [UI framework](developer/ui-sdk.md) provides declarative views, observable
state, layout, drawing and input for desktop windows and in-game overlays.
Start with [a first view](ui/first-view.md), then explore its focused topics and
linked API reference.

The [render pipeline](developer/render-pipeline.md) lowers those views to Blend2D
work, with retained resources, repair and relocation. [Profiling](developer/profiling.md)
and [optimizer quality](developer/optimizer-quality.md) explain how to measure it.

## Develop And Investigate

- [Contributor guide](DEVELOPER.md): build, test and find the owning subsystem.
- [Architecture](developer/structure.md): shared mechanisms and per-game policy.
- [Daemon](developer/daemon.md): lifecycle, storage and client contracts.
- [Research](research/index.md): verified findings, evidence and open questions.
- [Performance history](https://zeewanderer.github.io/retinue/research/benchmarks/): explore frame time, memory and other metrics by workload.

Generated API, command and protocol reference sits alongside the relevant guides
in the navigation. Search covers both. See [documentation authoring](developer/documentation.md)
to build the site locally or add a topic.
