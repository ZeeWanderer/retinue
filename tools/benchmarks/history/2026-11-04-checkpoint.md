# Last Commit To Current

Clean `cb0a0ce` versus the November 4 working tree, measured again in alternating
order. This is not the November 3 comparison with an intermediate tree. The new
optimizer-quality harness is test-only; these results cover accumulated production
changes, not the harness itself.

## Method

Eight fresh process pairs per suite, optimized saved binaries, i7-10700, affinity
CPUs 4-7, four rendering threads including the caller. Fixture hashes, workload
versions, build settings, actual threads and card counts match. No concurrent
builds/tests from this task; ordinary desktop activity remains uncontrolled.
Native completion is timed; compositor, network and game FPS are not.

Only unchanged workloads are compared. Foundry's full-window case redraws the
whole window with fixture cards, not the later captured-artwork/three-buffer
relocation trace. Relics measures the central page in a 2560x1440 shell. Rails
use an unpopulated center. Keep these boundaries when comparing timings.

## Results

Mean of eight process medians, milliseconds. Percentages are point estimates,
not acceptance verdicts.

| Workload | Resolution / Scale | Commit | Current | Change |
| --- | --- | ---: | ---: | ---: |
| Foundry full window | 1080p / 100% | 0.792 | 0.842 | +6.4% |
| Foundry full window | 1440p / 100% | 1.776 | 1.855 | +4.5% |
| Foundry full window | 1440p / 125% | 1.600 | 1.687 | +5.4% |
| Relics wheel | 1440p / 100% | 2.652 | 2.622 | -1.1% |
| Relics drag | 1440p / 100% | 3.490 | 3.367 | -3.5% |
| Relics wheel | 1440p / 125% | 1.691 | 1.670 | -1.2% |
| Relics drag | 1440p / 125% | 2.441 | 2.297 | -5.9% |
| Rail animation | 1440p / 100% | 0.776 | 0.768 | -1.0% |
| Rail animation | 1440p / 125% | 0.792 | 0.775 | -2.1% |

Relics drag has negative per-case 95% intervals on the median difference at both
scales: -0.233 to -0.011 ms at 100%, -0.202 to -0.076 ms at 125%. This supports a
local improvement, not a simultaneous guarantee across all cases. Wheel and rail
intervals cross zero.

Foundry full-window intervals also cross zero: -0.029 to +0.184 ms at 1440p/100%,
-0.018 to +0.196 ms at 125%. An eight-pair identical-current-binary control shifts
full-window medians from -3.0% to +3.7% across the matrix. Treat the observed
slowdown as **unconfirmed**, not a demonstrated source regression or a clearance.

The unchanged exploratory p95 screen, `max(0.1 ms, 5%)`, reports 5 passing and
13 inconclusive cases out of 18, no confirmed regression. The identical-binary
control has 2 passing and 6 inconclusive cases. Tails remain too noisy for broad
acceptance. No tolerance or historical baseline was changed.

125% and 200% remain in these old workloads so inputs match; they are not substitutes
for 120% measurements. New artwork-heavy tests cover 120% but have no equivalent
at `cb0a0ce`. Their results cannot be presented as last-commit speedups.

## Records And Reproduction

| Suite | Clean Commit | Working Tree |
| --- | --- | --- |
| Foundry | [Record](records/5a9388e1462b499ca7f67e89eb066cc2.json) | [Record](records/af079991a9dd4569ad14ff1e1505b0c1.json) |
| Relics | [Record](records/26c5aab537eb4d099ef1ced9a235a220.json) | [Record](records/7512d041f0634dfc9b01c23388c3f368.json) |
| Rails | [Record](records/8242ab5f26754daaa7600d81974f165d.json) | [Record](records/9245635afa364f86aa11fc9f4ef2637e.json) |
| Identical-binary control | [First label](records/770c2849bf5a49afaa4110a3e534e406.json) | [Second label](records/ca386d639f0246d0bd9cd6342523bd89.json) |

Ignored evidence: `research/retinue/benchmarks/head-current-20261104/` contains
snapshots, raw runs, `summary.json`, p95 verdicts and load samples. Clean snapshots
remain under `research/retinue/benchmarks/acceptance-20261103/head/`.

```bash
taskset --cpu-list 4-7 python3 tools/benchmarks/run.py pair \
  research/retinue/benchmarks/acceptance-20261103/head/gui-foundry/snapshot \
  research/retinue/benchmarks/head-current-20261104/gui-foundry \
  --output research/retinue/benchmarks/foundry-recheck --repeats 8 --threads 4
```

Use the current snapshot for both arguments for the control. Rebuild a new snapshot
with `make bench-prepare` for a subsequent fix. Follow-up state and closure criteria
live in the [single follow-up list](follow-ups.md), not new chronological reports.

## Earlier Direct Renderer

The October 25 pre-composition executable and raw results remain under
`research/retinue/ui-composition/migration/`. `RESULTS.md` records two separate
three-run batches; both include layout, submission and native completion, not
presentation. Full-window Foundry scrolling, median of run medians:

| Resolution / Scale | Initial Batch | Later Repeat |
| --- | ---: | ---: |
| 1080p / 100% | 1.960 ms | 1.511 ms |
| 1440p / 100% | 4.066 ms | 3.164 ms |
| 1440p / 125% | 3.348 ms | 2.706 ms |

Four threads, 240 synthetic cards, 70 visible at 1440p/100%, not a live player
artwork workload. A game was running; those batches were not controlled against
today's measurements. They establish historical frame cost, not a causal SDK
overhead or speedup. Older sub-millisecond rail-only numbers are not populated
GUI frames.
