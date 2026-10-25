# Blend2D Mask Issues

Reviewed 2026-10-23 against Blend2D **0.21.3**, commit
`58ca9460b4138af6e793184682e9324e0a8053b2`, on Linux x86-64.
These are findings about the pinned version, not a claim about the latest
upstream code or existing issue reports.

## Queued Mask Lifetime: Bug

**Symptom:** asynchronous A8 mask draws retain their source allocation after the
context finishes. Repeated glyph rendering can leak memory despite cache eviction.

**Reproduce:** create a PRGB32 target and a context with two rendering threads;
wrap an A8 allocation with `BLImage::create_from_data` and a counted destroy callback.
Submit a solid-colour `fill_mask`, reset the caller's mask image, then end/destroy
the context. The callback should run exactly once; the unpatched path retains it.
The allocation must remain valid until the callback runs.

**Cause:** `fill_clipped_box_masked_a<kAsync>` increments the image reference count
but omits both the retained-mask flag and the cleanup-queue mark. The release loop
requires those markers. See the pinned [enqueue code][mask-enqueue] and
[batch cleanup][mask-cleanup].

**Current fix:** insert these lines before retaining the mask:

```cpp
command->add_flags(RenderCommandFlags::kRetainsMaskImageData);
ctx_impl->worker_mgr().command_appender().mark_fetch_data();
```

Our [CMake workaround](../../crates/retinue-ui/native/blend2d-mask-retention.cmake)
compiles a corrected source copy, leaving the vendor checkout untouched. It stops
the build if the expected code changes. The regression test
`queued_images_and_masks_live_until_completion_or_drop` in
[blend2d.rs](../../crates/retinue-ui/src/painter/blend2d.rs) covers explicit finish,
context drop and 1/2/4 threads. `make test-gui` runs it against the **patched** build;
an upstream reproduction must use stock Blend2D without our CMake replacement.

**Upstream:** suitable for a bug report with the lifetime reproducer and patch.

## Fractional Mask Positioning: Missing Support

**Symptom:** cached glyph rendering fails during fractional scroll/translation.
With a valid, visible A8 mask and target, this illustrates the failing case:

```cpp
ctx.translate(0.0, 0.5);
BLResult result = ctx.fill_mask(BLPointI(0, 0), mask, BLRgba32(0xFFFFFFFF));
// Pinned implementation returns BL_ERROR_NOT_IMPLEMENTED.
```

The [mask dispatch][mask-transform] implements the pixel-aligned path; fractional
placement and more general transforms reach explicit TODOs. This is separate from
the reference leak and is not restricted to multithreaded rendering.

**Current workaround:** keep animation state fractional but snap displayed scroll
origins to physical pixels. Paint, hit testing and accessibility share that origin.
Rasterize fonts/assets at the final scale instead of scaling the context transform.

**Upstream:** ask about transformed mask support; do not report it as a regression
without evidence of a previously supported version.

[mask-enqueue]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L3140-L3174
[mask-cleanup]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L998-L1020
[mask-transform]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L3593-L3660
