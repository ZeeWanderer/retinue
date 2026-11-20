# Contributing

Start with [build workflows](developer/workflows.md) and [testing](developer/testing.md).
The [project structure](developer/structure.md) defines ownership: clients handle
presentation and input; the daemon owns canonical data and domain computation.

## Choose A Subsystem

| Work | Start here |
| --- | --- |
| Desktop or overlay UI | [UI framework](developer/ui-sdk.md), [first view](ui/first-view.md) |
| Painting, repair and caching | [Render pipeline](developer/render-pipeline.md), [profiling](developer/profiling.md) |
| Services and client contracts | [Daemon](developer/daemon.md), [health reporting](developer/health.md) |
| Commands and data queries | [CLI implementation](developer/cli.md), [query engine](developer/query-language.md) |
| Game observers | [Companion lifecycle](developer/companion.md), [player data](developer/player-data.md) |
| Warframe catalogs and planning | [Knowledge](developer/knowledge.md), [Forma solver](developer/forma_plan.md) |
| Research | [Evidence and findings](research/index.md), [inspection tools](wfinspect.md) |

Each subsystem's navigation groups its guides and generated reference together.
Use the API links in a guide for signatures; use the related guides on an API page
for its intended usage and constraints.

## Make A Change

1. Find the owner using the [GUI](developer/gui-organization.md),
   [overlay](developer/companion-presentation.md) or [system](developer/structure.md) source map.
2. Follow the [feature checklist](developer/adding_features.md). Update affected
   clients, contracts and tests together.
3. Run the affected correctness tests and Common Test. Use the
   [benchmark workflow](../tools/benchmarks/README.md) for performance claims.
4. Update the relevant topic or API comments. Follow
   [documentation authoring](developer/documentation.md); do not edit generated output.
