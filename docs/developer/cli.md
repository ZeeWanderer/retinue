# CLI Architecture

## Commands

`retinue_cli` composes platform commands with game subtrees. `wf_cli` supplies the Warframe
tree and shared dispatch context. `wfcli` and `poe2cli` select their scope before parsing;
they do not maintain separate command definitions. Each command module declares
its OTP `argparse` specification beside its handler: arguments, types, defaults, subcommands,
summary and optional notes. Handlers receive parsed maps, not argv.
Enum types supply completion values; use argument `completion` hints for open-ended values.

- `retinue_cli_args` supplies shared argument definitions and contextual help adaptation.
- `wf_catalog_cli` handles focused export and knowledge queries.
- `wf_query_cli` handles unified queries and the player view.
- `wf_worldstate_cli` handles one-shot requests and watch-spec syntax;
  `wf_worldstate_watch_cli` owns subscription lifetime.
- `wf_update_cli` is the update entry point.

Add a subcommand when it narrows an operation; use options for composable modifiers.
Define each argument once. Do not add separate option registries, hand-written argv parsers or
copied help tables. Generated help and Bash completion consume the same tree.
Keep domain validation, such as mutually exclusive scopes, in the owning handler.

Follow the [Command Line Interface Guidelines](https://clig.dev/#subcommands):
organize commands by user tasks, use consistent verbs, and show common operations
first. Platform operations belong at the root; game operations belong in one
game subtree. `doctor` answers whether observed functionality works; `diagnostics`
holds captures, watches and evidence. Do not create another nested diagnostics
tree. Legacy spellings reuse the canonical command specifications as hidden
aliases, never copied handlers. Add examples and actionable next steps to errors.

The query DSL reaches the daemon uncompiled. Focused flags become typed filters; free query text
stays opaque. Preserve literal tails after `--`. Reuse behavior through typed functions or shared
clients, never by rebuilding argv and invoking another parser. Matching, sorting, pagination and
`dataset=` interpretation belong to daemon query services.

## Help And Completion

`help COMMAND`, `COMMAND help`, `COMMAND --help` and `COMMAND -h` resolve the same scope.
Values named `help` remain values. Typo correction requires terminal stdin and stdout and respects
`--no-suggest-prompt`.

`retinue_completion` generates static Bash maps from the complete Retinue tree and a builtin-only
completion function that selects the initial scope from the executable name. Builds stage
them under `share/bash-completion/completions` for lazy loading. Keep Bash 5.3 `compgen -V`:
neither shell initialization nor completion may start an Erlang VM. Tests execute the generated
script with external commands unavailable.

## Output

Usage errors use stderr and exit 2; failed operations exit 1. Help and successful data use stdout.
Batch commands retain successful results while returning failure if any job fails.

Health reports distinguish failure from waiting, unsupported and untested checks.
An open connection is not evidence that a memory adapter ran. Doctor is read-only:
no auto-start, repair, capture, update or input injection. Incident history is not
live health. `--json` retains these distinctions for automation.

Use `retinue_table` and `retinue_tty` for terminal layout. Shared schema modules define columns;
CLI presentation modules define block ordering. Focused and unified queries share
`wf_exports_format` and `wf_knowledge_format`.

Catalog JSON uses explicit field types in `wf_catalog_json`. Never infer strings from integer
arrays or empty lists. Preserve nested source JSON types. Use `retinue_data_extract` for dot paths
and `retinue_text` for shared string conversion.

## Verification

Test specifications and handlers, then actual child-process stdout, stderr, exit codes, literal
arguments and partial failures. Keep help/completion coverage tree-driven so new commands join
the same checks. Architecture boundaries and interface versioning are in
[daemon.md](daemon.md); feature wiring is in [adding_features.md](adding_features.md).
Shared check reporting and retention are described in [Health evidence](health.md).
