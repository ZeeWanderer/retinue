# Blend2D Integration Issues

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

Our [CMake workaround](../../crates/retinue-render/native/blend2d-mask-retention.cmake)
compiles a corrected source copy, leaving the vendor checkout untouched. It stops
the build if the expected code changes. The regression test
`queued_images_and_masks_live_until_completion_or_drop` in
[blend2d.rs](../../crates/retinue-render/src/painter/blend2d.rs) covers explicit finish,
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

## Color Glyph Runs: Capability Gate

Retinue keeps text on native Blend2D glyph runs. The current [face
validator](../../crates/retinue-render/src/text/font.rs) accepts static outline
fonts, not color or bitmap faces. Reference overlays can use host color emoji;
prefer a compatible monochrome outline for a missing glyph, or the primary
face's missing-glyph box. Do not discard the rest of the line or add a second
rasterizer. This is an integration limitation, not a reported upstream bug.

Bypassing the validator on the pinned revision, a positioned U+1F600 glyph
from Noto COLRv1 opened, submitted and finished successfully but left a
transparent target unchanged. The same native bridge and target painted the
monochrome Symbola outline. The check is conservative at face granularity;
this result does not show that every glyph in a mixed font lacks an outline.

On a Blend2D update that adds native color glyph rendering, test a mixed
text/emoji run with a real color font: check color pixels, face/index selection,
editor advances and queued draw lifetime. Relax only the tested format in the
validator. Successfully opening a color font does not establish color rendering.

[mask-enqueue]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L3140-L3174
[mask-cleanup]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L998-L1020
[mask-transform]: https://github.com/blend2d/blend2d/blob/58ca9460b4138af6e793184682e9324e0a8053b2/blend2d/raster/rastercontext.cpp#L3593-L3660
