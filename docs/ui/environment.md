# Inherited Environment

[api:ViewProperties]: derive@retinue_ui_macros::ViewProperties

Environment supplies dependencies through a subtree. A child uses the nearest
definition; an override affects its descendants, not its siblings. Use named keys
for distinct roles with the same value type, or type-keyed model injection for a
single shared service/model of that type.

Declare inherited members with `ViewProperties`, or use
`context.environment()` directly. `.environment` supplies a named value and
`.environment_model` supplies a shared model. These modifiers do not add layout
boxes. The detailed [SDK contract](../developer/ui-contracts.md#inherited-environment)
lists the explicit lookup and replacement methods.

## Defaults Are Not Global Variables

An [EnvironmentKey](https://zeewanderer.github.io/retinue/reference/rust/retinue_ui/environment/trait.EnvironmentKey/)
declares a type and default policy, not process-global mutable
storage. Each root owns its defaults. Windows, tests and subtrees may override
them. Pass cheap model handles instead of cloning complete datasets on lookup.

For type-keyed models, `model::<T>()` is optional and `require::<T>()` reports a
missing dependency. Do not silently construct an empty replacement for a required
model. `remove_model` exposes an inherited provider again.

## Provider And Field Changes Are Separate

Replacing the resolved model invalidates consumers of that provider so their
subscriptions can move to the new model. Editing a field invalidates only views
that read that field. An ancestor update hidden by a nearer override does not
invalidate that branch. Consumers of a missing/default value still notice when
a new nearer provider appears.

Environment is optional. Ordinary model members and standalone
[observation](state.md#track-only-what-is-read) use the same state runtime.
