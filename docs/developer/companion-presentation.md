# Companion Presentation

Overlays follow the same feature-oriented organization as the
[desktop GUI](gui-organization.md). Named views and their models live together;
window hosts, workers and data contracts have separate owners.

## Warframe

Paths below are relative to `apps/wfcompanion/src/`.

| Task | Entry point |
| --- | --- |
| Surface, focus and input lifecycle | `presentation/overlay_host.rs` |
| Scene and interaction state | `presentation/overlay_model.rs` |
| Frame preparation, repaint and hit regions | `presentation/renderer.rs` |
| Screen dispatch | `presentation/screens.rs` |
| Ducat tooltip | `presentation/ducats/tooltip_view.rs` |
| Relic rewards | `presentation/relic/reward/reward_view.rs` |
| Reward card layout | `presentation/relic/reward/card_layout.rs` |
| Relic suggestions | `presentation/relic/suggestion/suggestion_view.rs` |
| Image preparation and leases | `domain/media.rs` |

Each relic feature keeps its drawing code in `mod.rs`, its named SDK views
beside it, and geometry projections in `*_geometry.rs`. These projections are
results of layout, not alternative layout engines. `PaintOptions` and
`StatusParams` are drawing inputs, not SDK `View` implementations.

Game observation, capture, daemon communication and reload remain outside
presentation. The existing `relic` and `ducats` modules own their respective
observation/request flows; similarly named presentation folders only draw them.

## Path Of Exile 2

Paths below are relative to `apps/poe2companion/src/`.

| Task | Entry point |
| --- | --- |
| Surface, focus, shortcuts and task delivery | `presentation/overlay_host.rs` |
| Evaluate layout | `presentation/evaluate/evaluate_view.rs` |
| Item layout | `presentation/evaluate/item_view.rs` |
| Filters, selection and request state | `presentation/evaluate/evaluate_model.rs` |
| Painting and control geometry | `presentation/evaluate/panel.rs` |
| Offline fixture rendering | `presentation/preview.rs` |
| Trade and clipboard jobs | `domain/trade.rs` |
| Currency image loading | `domain/currency.rs` |
| Rate budget and preferences | `domain/budget.rs`, `domain/preferences.rs` |
| Evaluation and listing records | `data/evaluation.rs` |

`EvaluatePanel` is a paint/input controller. `EvaluateView` and `ItemView` are
the declarative entry points. Their model requests currency images by identity,
size and generation; the image worker does not import presentation state.

## Boundaries

- `*_view.rs` implements `View`; `*_layout.rs` implements `Layout`.
- Models sit beside their feature's views. No parallel global view-model tree.
- Custom Canvas painting remains supported inside the retained SDK path.
- Domain services perform client-side work, not canonical game calculations.
- New companions need only the folders they use; a launch-only companion does
  not need an empty presentation hierarchy.

Native hosts share SDK buffer submission and renderer repair history. Game-specific
lifecycle and input policy stay in the companion; see [native hosts](ui-sdk.md#native-hosts).
App-local foundation helpers follow the [Core guidance](structure.md#app-local-foundations),
not a mandatory additional layer.

Verify changes with [overlay previews](../../tools/ui-reference/README.md) and
the companion test targets in [workflows](workflows.md).
