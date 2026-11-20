# State And Bindings

Use [`State<T>`](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/state/struct.State/)
for a single observable value and
[observable](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui_macros/attr.observable/) for related
fields. The model is a shared handle; it does not need an environment or view
tree to exist.

```rust
use retinue_ui::prelude::*;

#[observable]
struct Equipment {
    name: String,
    rank: u32,
    #[observation(ignore)]
    request_generation: u64,
}

let equipment = Equipment::new("Karak".into(), 0, 0);
let same_equipment = equipment.clone();
equipment.rank.set(1);
assert_eq!(same_equipment.rank.get(), 1);
let rank_editor = equipment.rank.binding();
```

`get()` clones the value; `read()` borrows it. `set()` suppresses equal changes.
`replace()` and `update()` support values without equality. `binding()` exposes
an editable source without duplicating its value. Merely creating a binding
does not subscribe; reading it does. Ignored fields do not notify observers.

## Own Models Deliberately

The application may create a model and pass it into a view. A view may instead
retain one through `context.owned`, or use `ViewProperties` for typed fields.
Use [environment](environment.md) for a dependency shared through a subtree.
None of these changes the field-observation rules.

Keep async work, request cancellation and persistence with the owning service or
model/controller. Views describe state; actions initiate changes. Publish worker
results on the UI thread. Model data should not be a second copy of the daemon's
canonical computation.

## Track Only What Is Read

Observation follows actual field reads, including reads inside computed methods.
Changing one field invalidates its readers, not every consumer of the model.
`Items<T>` additionally separates row state from collection order: editing a row
does not invalidate unrelated rows or order consumers.

Outside a view, retain
[Observation](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/state/struct.Observation/)
and evaluate through `track`. Its
callback schedules reevaluation; it must not render synchronously. Each successful
evaluation replaces dependencies. Dropping the observer unsubscribes it. Tracking
does not extend through `await` or arbitrary threads.

Animation transactions attach presentation intent to mutations. They are not
database transactions and provide no rollback. See [animation](animation.md) and
the [observation contract](../developer/ui-contracts.md#observation-contract).
