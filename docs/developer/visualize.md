# Visualization Pipeline

## Entry points

- CLI: `apps/retinue_cli/src/forma/wfcli_visualize.erl`
- Rendering: `apps/retinue_cli/src/forma/wfcli_forma_visualizer.erl`

## Where to edit what

- CLI flags/usage/errors: `apps/retinue_cli/src/forma/wfcli_visualize.erl`
- HTML/SVG rendering: `apps/retinue_cli/src/forma/wfcli_forma_visualizer.erl`

## Runtime notes

- `wfcli visualize` accepts `--viz-config` to render the input config layout, and `--config` to override the config path when the plan YAML points elsewhere.
