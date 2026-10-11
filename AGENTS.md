# AGENTS

Agent-only notes. User-facing docs start in `README.md` and continue in the linked
guides under `docs/`.

## Workflow
- Use `$caveman`. Preserve full technical meaning while removing token waste.
  Keep code, exact errors, commit messages, and user-facing documentation in their
  normal forms.
- Save large tool outputs to files; return only targeted excerpts or summaries.
  Select output before running commands instead of relying on truncation.
- Keep tests in sync with expected outputs; CT suite lives under `apps/retinue_cli/test`.
- Run tests after code, fixture, build, or behavior changes (at minimum
  `./scripts/test-quiet ct`). Documentation-only edits do not require tests.
- Run tests elevated, outside the Codex sandbox; socket and Erlang distribution
  tests require host namespaces.
- Use `./scripts/test-quiet eunit` for EUnit. It suppresses passing noise and keeps
  a full `/tmp` log only on failure; use direct `rebar3` only when debugging.
- Run `make dev-erlang` before testing `./wfclid`; run `make prod-erlang` before
  testing `./wfcli`. Both commands refresh the escript and relx daemon release.
- Run `make dev-companion` or `make prod-companion` after companion code, native
  bridge, asset, or build metadata changes.
- Follow the lockstep compatibility policy in `docs/developer/daemon.md`.
- Review game integrations for update resilience. Discover build-dependent bindings;
  do not hardcode per-build offsets as the production strategy. Validate layout
  assumptions and test relocation, ambiguity and changed structures.
- After tests pass, manually exercise updated CLI commands yourself (not just via tests).
- Add new tests whenever new functionality is added; update fixtures alongside expected outputs.
- Keep docs lean and standalone: current behavior/contracts, no conversation context or irrelevant
  absences.
- `rebar3 ct` emits a warning about `-compile(export_all)` in `apps/retinue_cli/test/wfcli_forma_plan_SUITE.erl`; tests still pass.

## Reference
- See `README.md` for the command overview and its linked user guides for detailed
  usage, configuration, query, data-source, and daemon notes.
