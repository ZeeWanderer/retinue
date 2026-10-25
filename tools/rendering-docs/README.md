# Rendering Reference

Run `./tools/rendering-docs/generate` with Doxygen installed. It generates the
complete public API reference from the pinned Blend2D headers under
`research/rendering/docs/blend2d-pinned/`. Open `html/index.html`; the source
revision and generation log are beside it.

Rerunning is cheap: source, generator or Doxygen changes refresh the output.
This is an optional research tool, not an application build dependency.
Use the pinned headers for implementation details. Online guides may describe a
different version.
