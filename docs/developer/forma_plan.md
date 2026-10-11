# Forma-Plan Pipeline

## Entry points

- CLI: `apps/retinue_cli/src/games/warframe/forma/wfcli_forma_plan.erl`
- Daemon queue: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_service.erl`
- Public solver API: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_planner.erl`
- Search: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_search.erl`
- Cost and validity rules: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_rules.erl`
- Mod assignment: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_assignment.erl`
- Item model: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_model.erl`
- Mod lookup: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_mod_db.erl`
- YAML loader: `apps/retinue_daemon/src/games/warframe/forma/wfcli_forma_config.erl`
- Visualization: `apps/retinue_cli/src/games/warframe/forma/wfcli_forma_visualizer.erl`

## Where to edit what

- CLI flags/usage/errors: `apps/retinue_cli/src/games/warframe/forma/wfcli_forma_plan.erl`
- Queueing and replies: `wfcli_forma_service`
- Candidate generation and search: `wfcli_forma_search`
- Capacity, cost, and tiebreak inputs: `wfcli_forma_rules`
- Mod-to-slot matching: `wfcli_forma_assignment`
- Item/build normalization: `wfcli_forma_model`
- YAML parsing and errors: `wfcli_forma_config`
- Mod lookup source: `wfcli_forma_mod_db`
- YAML, HTML, and SVG output: the CLI modules above.

## Concurrency and search

The daemon serializes plan requests because one solver run can use every online scheduler.
The solver first finds a serial incumbent, splits the remaining frontier into bounded weighted
tasks, runs at most one worker per scheduler, and merges results deterministically. Search
budgets are split by estimated task weight.

Budget exhaustion is not itself failure when an incumbent exists. The solver attempts a
second bounded search below the incumbent cost; absence of a cheaper plan certifies the result.
Without certification it still returns the best valid plan and logs a warning. It returns
`search_budget_exhausted` only when no valid incumbent exists.

## Tests and fixtures

- Forma plan tests live in `apps/retinue_cli/test` (see `wfcli_forma_plan_SUITE.erl`).
- Test configs include the real `wisp.yml` regression and an independent three-build case.
- Sample config lives under `docs/forma_plan.example.yml`.
