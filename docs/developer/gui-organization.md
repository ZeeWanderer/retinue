# GUI Source Map

`apps/wfgui` is the Rust desktop client. The SDK is `crates/retinue-ui`;
Blend2D execution and paint optimization belong to `crates/retinue-render`.
The companions use the same boundaries; see their
[presentation source map](companion-presentation.md).

## Start Here

| Task | Entry point under `apps/wfgui/src/` |
| --- | --- |
| Window lifecycle, task delivery, capture arguments | `app.rs` |
| Window title and three-rail layout | `presentation/shell/shell_view.rs` |
| Page selection and shared actions | `presentation/page_model.rs` |
| Foundry layout | `presentation/foundry/foundry_view.rs` |
| Inventory, Mastery and Relic layout | `presentation/listing/listing_view.rs` |
| Build Planner | `presentation/build_planner/build_planner_view.rs` |
| Market | `presentation/market/market_view.rs` |
| Settings | `presentation/settings/settings_view.rs` |
| Right rail | `presentation/right_rail/declaration/rail_view.rs` |
| Reusable artwork and text | `presentation/widgets/` |
| Session, preferences and media services | `domain/` |
| Plain models and serialized values | `data/` |

## Views And State

A `*_view.rs` file contains the named `View` and its `body`. A `*_layout.rs`
implements a custom `Layout`; a `*_model.rs` contains the feature's observable
state. They sit beside each other. Related subviews may share a feature folder.
`mod.rs` wires the feature together and owns its controller where needed.

Views describe layout and read observable state. Controllers translate input and
task results into model changes. Tasks, transport and persistence live outside
view evaluation. Shared services enter through the inherited environment;
local models are owned by their feature. Do not introduce a singleton just
because several views use the same service.

`ControlHost` keeps native editing, focus and application control state. Its
`view_host` mounts a `ViewTree`, including custom `PaintSlot` primitives. There
is no alternate manual scene builder. `Canvas`, `PixelCanvas` and direct paint
programs remain valid lower-level tools within the same SDK and renderer.

## Adding A Feature

1. Add its plain data contract in the owning protocol/core when needed.
2. Put shared client work in `domain`, and feature state beside the view.
3. Start layout in the named view file. Use a custom Layout or Canvas where it
   expresses the behavior more clearly than composed primitives.
4. Test state transitions and pointer/keyboard behavior; compare full and
   incremental repaint output at fractional scales.

Use [GUI captures](../gui.md) and [workflows](workflows.md) for verification.
