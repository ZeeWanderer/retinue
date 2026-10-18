# Development Tools

- [`ui-reference`](ui-reference/README.md): shared headless browser tooling with
  separate AlecaFrame and PoE Overlay II adapters. `make previews` renders all
  supported games; `previews-wf` and `previews-poe2` select one.
- [`ghidra`](ghidra/README.md): generic executable analysis and evidence export for
  `retinue-inspect`. Report identifiers remain stable for retained research data.
- `icon-optics`: shared icon sizing utility; `foundry.json` is a Warframe preset.
- [`wf-luau-decompiler`](wf-luau-decompiler/README.md): Warframe-specific Luau fork,
  kept as a pinned submodule.
- `staging`: shared atomic build-install helper.

Keep provider-specific fixtures and bootstrap code beside their adapter. Put only
reusable mechanisms in shared helpers. Downloaded packages, proprietary assets and
build-specific evidence belong under ignored `research/`, not in tools.
