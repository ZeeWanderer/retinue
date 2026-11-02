# Testing

Tests belong to the code they protect. Classify their **scope**, **environment**
and **purpose** separately: a multi-process contract test can still be isolated
and deterministic; a small memory probe can require a live game.

## Placement

| Content | Location |
| --- | --- |
| Erlang unit and component/contract tests | `apps/<owner>/test/` |
| Rust implementation tests | Beside the implementation, usually `tests.rs` |
| Rust public API tests | The owning crate's `tests/` |
| Cross-application scenarios | `test/integration/` |
| Shared Erlang test support | `test/support/` |
| Shared fixture corpus | `test/fixtures/` |
| Tool tests | Beside the tool |
| Benchmark catalog, collection and comparison | `tools/benchmarks/` |

Use Cargo, EUnit and Common Test as the test runners. Make provides common entry
points, not another test framework. Keep private benchmark implementations with
their code; do not expose internals merely to move them into `benches/`.

Shared Erlang fixtures use `retinue_test:fixture/1`, independent of Common Test's
working directory. Fixtures are read-only. Copy inputs into `priv_dir` or a unique
temporary directory before testing mutations. Keep retained reports in ignored
`research/`, separate from temporary runtime state.

## Running

`make test` runs automated correctness checks without a game, a user's daemon,
compositor input or performance thresholds. Building dependencies can require
network access. Socket tests require host namespaces, not necessarily root.

| Target | Coverage |
| --- | --- |
| `test-erlang` | All OTP applications and cross-app EUnit/Common Test suites |
| `test-native` | Protocol, client, native/game adapters and inspector |
| `test-ui` | Renderer, SDK and macros; headless, overlay and desktop feature sets |
| `test-companion` | The three companion applications |
| `test-gui` | WFGUI application |
| `test-tools` | Benchmark/report tools, integration helpers and documentation tools |
| `test-staging`, `test-build` | Installation and build-system contracts |
| `test-integration` | Staged PoE2 observer/daemon/CLI scenario, isolated storage and network |

Focused runs remain native:

```bash
./scripts/test-quiet eunit --application retinue_daemon
./scripts/test-quiet ct --suite apps/retinue_daemon/test/retinue_poe2_trade_SUITE.erl
make test-ui-sdk UI_TEST='--test modifiers'
make test-gui GUI_TEST=presentation::foundry::
make test-poe2companion COMPANION_TEST=clipboard
```

Live checks are explicit: `make test-live LIVE_TEST=gui-session` needs a running
daemon. Other opt-in checks and their prerequisites appear in `make bench-list`.
Missing prerequisites mean **unverified**, never success. `#[ignore]` alone is
not classification: register ignored native checks in the catalog.

## Correctness And Work Budgets

Prefer the smallest isolated test that demonstrates the behavior. Test
cross-process contracts at their real boundary as well as testing their parsers.
Use fixed data, scripted input and controlled time where practical.

Optimization guarantees belong in ordinary correctness tests: unchanged input
avoids layout/decode work; partial repair matches fresh painting; subscriptions
and retained resources are released. Assert meaningful work bounds, not incidental
instruction counts or elapsed milliseconds. Rendering comparisons also need
independent geometry/pixel expectations: two paths sharing a bug can agree.

## Performance Regressions

Use the [benchmark guide](../../tools/benchmarks/README.md). Maintained headless
scenarios cover both overlays, GUI rail animation, Foundry/Relic scrolling and
scrollbar dragging. Exploratory cache/repair strategies and live probes remain
separate. Share fixtures and scenario drivers where possible, not assertions in
timed sections.

- Build optimized binaries before measuring; include native completion, not just
  queued submission. Keep setup, validation and cold/warm samples distinguishable.
- Preserve raw samples, workload/fixture identity, source/binary provenance,
  compiler/configuration, CPU affinity, thread budget, cadence and cache state.
- Repeat in fresh processes. Prefer balanced/interleaved A/B runs; frame samples
  within one process are not independent benchmark repetitions.
- Compare latency, CPU at equal useful work/cadence, and memory separately. RSS
  is not live allocations. A faster uncapped loop can consume more CPU without
  becoming less efficient. Headless timings do not measure game impact.
- Review practical tolerances per workload/case and metric. No automatic baseline
  replacement and no universal percentage. Changed fixtures/configuration need
  explicit review, not a misleading timing comparison.
- Verdicts are `pass`, `regression`, `inconclusive` or `invalid`. Too few runs or
  an uncertainty interval crossing the tolerance is inconclusive. Missing cases,
  changed configuration or an unfinished run is invalid.

Shared CI gates correctness and deterministic work budgets. Statistical timing
gates need a controlled runner and calibrated policy; busy workstation results
are advisory. Retain evidence for suspected regressions and reproduce them before
changing the renderer. Current tooling does not claim a calibrated timing gate.

Keep selected compact measurement records in `tools/benchmarks/history/`, keyed by
measurement ID and compatible workload/environment. Source commits are provenance,
not a requirement to benchmark every commit. Baseline replacement needs an explicit
reason; raw evidence remains ignored. Use [external profiling](profiling.md) to
explain results, not as a substitute for unprofiled performance measurements.

Criterion is suitable for new isolated Rust microbenchmarks. Nextest can provide
process isolation/resource groups if native Cargo scheduling becomes inadequate;
it is not required merely for test organization. Neither replaces the frame,
compositor or live-game workloads.

## References

- [Test scope versus size and hermeticity](https://abseil.io/resources/swe-book/html/ch11.html)
- [Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html)
- [Criterion measurement analysis](https://bheisler.github.io/criterion.rs/book/analysis.html)
- [Rebar3 EUnit discovery](https://rebar3.org/docs/testing/eunit/)
