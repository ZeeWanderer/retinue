# Run the view demo

Explore a retained grid, tab transitions, hit testing and asynchronous artwork
in a standalone window. No game, daemon, or downloaded images are required.

```sh
make view-demo
make view-demo VIEW_ARGS='--scale 1.25'
```

Scroll the grid or change tabs. Space changes an amount, R replaces artwork,
and C evicts composed rasters. These interactions exercise different updates:
a translation changes placement, while replacing artwork changes the pixels
the renderer may reuse.

## Capture and measure

```sh
make view-demo VIEW_ARGS='--capture'
make bench-views
```

Captures include geometry as well as images. Set `VIEW_OUTPUT` to choose the
benchmark output directory. Use [the benchmark guide](../../tools/benchmarks/README.md)
for comparable runs. This demo measures neither a complete application nor
the compositor's effect on a running game.

See [the example source](../../crates/retinue-ui/examples/views/),
[composition](ui-composition.md), and [the render pipeline](render-pipeline.md).
