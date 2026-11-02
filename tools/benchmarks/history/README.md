# Accepted Measurements

Keep selected compact reports here, not every run. Measure relevant changes and
release checkpoints. Raw frame samples, traces and executable snapshots stay in
ignored `research/` or a separate artifact store.

```bash
python3 tools/benchmarks/history.py archive path/to/report.json --note 'Reason for retaining this run'
python3 tools/benchmarks/history.py baseline MEASUREMENT_ID --reason 'Reason for accepting this baseline'
python3 tools/benchmarks/history.py compare path/to/candidate.json --policy path/to/policy.toml
```

Reports are immutable. Series match workload, fixtures, cases, machine and build
configuration; the source commit is provenance, not the series identity.
`baselines.json` changes only through explicit selection. Review that diff with
the report; Git preserves prior baseline decisions. Do not advance baselines
automatically or accept a sequence of small regressions unnoticed.

Dirty-worktree runs can be archived but cannot be selected as baselines. No
baseline is supplied until a clean, representative measurement is reviewed.
