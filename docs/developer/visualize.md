# Visualization Pipeline

## Entry points

- CLI flags, usage and errors: `wf_visualize.erl`.
- HTML and SVG rendering: `wf_forma_visualizer.erl`.

## Runtime notes

- `wfcli visualize` accepts `--viz-config` to render the input config layout, and `--config` to override the config path when the plan YAML points elsewhere.
