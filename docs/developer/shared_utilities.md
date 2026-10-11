# Shared Utilities

`retinue_core` and `wfcore` are process-free OTP libraries. Every BEAM module has one application
owner; compiling the same source into CLI and daemon applications creates module clashes in xref
and ambiguous debugger paths.

`retinue_core` owns platform helpers:

- `retinue_paths`: XDG roots, game namespaces, private runtime directory validation.
- `retinue_protocol`: handshake and interface contracts.
- `retinue_build`: staged artifact flavor, install root, and update paths.
- `retinue_text`, `retinue_time`, `retinue_data_extract`, `retinue_incidents`: value and read-only data helpers.

`wfcore` depends on it and owns `wf_protocol` dataset/request contracts, `wf_paths`,
column schemas, polarity and worldstate diff helpers.

Do not add fetching, persistence, processes, CLI parsing, or rendering to `wfcore`. Two callers
inside one application do not justify moving code here; a stable contract needed by both
applications does.

Keep behavior inside its owning application. Prefer a focused module when behavior has one owner
and independent tests; do not split files only to reduce line count. Avoid compatibility wrappers
after a move unless an external contract requires one.

CLI-only examples: `retinue_tty`, `retinue_table`, `retinue_cli_args`.

Daemon-only examples: `retinue_data_cache`, `wf_query_parse`, `wf_entity_*`.

Exported inter-module functions require `-spec` and terse `-doc` text where purpose or ownership is
not obvious. Shared behavior changes require focused EUnit or Common Test coverage.
