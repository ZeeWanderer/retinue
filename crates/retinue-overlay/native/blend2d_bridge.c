#include <blend2d.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct RetinueBlendPainter {
  BLImageCore image;
  BLContextCore context;
} RetinueBlendPainter;

RetinueBlendPainter* retinue_bl_painter_create(
    uint8_t* pixels,
    int width,
    int height,
    intptr_t stride,
    BLResult* result_out) {
  RetinueBlendPainter* painter = NULL;
  BLResult result = BL_SUCCESS;

  if (!pixels || width <= 0 || height <= 0 || stride < (intptr_t)width * 4) {
    result = BL_ERROR_INVALID_VALUE;
    goto done;
  }

  painter = (RetinueBlendPainter*)malloc(sizeof(RetinueBlendPainter));
  if (!painter) {
    result = BL_ERROR_OUT_OF_MEMORY;
    goto done;
  }

  result = bl_image_init_as_from_data(
      &painter->image,
      width,
      height,
      BL_FORMAT_PRGB32,
      pixels,
      stride,
      BL_DATA_ACCESS_RW,
      NULL,
      NULL);
  if (result != BL_SUCCESS) {
    free(painter);
    painter = NULL;
    goto done;
  }

  result = bl_context_init_as(&painter->context, &painter->image, NULL);
  if (result != BL_SUCCESS) {
    bl_image_reset(&painter->image);
    free(painter);
    painter = NULL;
    goto done;
  }

  result = bl_context_set_hint(
      &painter->context,
      BL_CONTEXT_HINT_PATTERN_QUALITY,
      BL_PATTERN_QUALITY_BILINEAR);
  if (result != BL_SUCCESS) {
    bl_context_end(&painter->context);
    bl_image_reset(&painter->image);
    free(painter);
    painter = NULL;
  }

done:
  if (result_out) {
    *result_out = result;
  }
  return painter;
}

BLResult retinue_bl_painter_destroy(RetinueBlendPainter* painter) {
  if (!painter) {
    return BL_SUCCESS;
  }
  BLResult result = bl_context_end(&painter->context);
  bl_image_reset(&painter->image);
  free(painter);
  return result;
}

BLResult retinue_bl_clear(RetinueBlendPainter* painter) {
  return bl_context_clear_all(&painter->context);
}

BLResult retinue_bl_fill_round_rect(
    RetinueBlendPainter* painter,
    double x,
    double y,
    double width,
    double height,
    double radius,
    uint32_t rgba32) {
  BLRoundRect rect = {x, y, width, height, radius, radius};
  return bl_context_fill_geometry_rgba32(
      &painter->context, BL_GEOMETRY_TYPE_ROUND_RECT, &rect, rgba32);
}

BLResult retinue_bl_stroke_line(RetinueBlendPainter* painter, double x0, double y0,
                         double x1, double y1, double width, uint32_t rgba32) {
  BLLine line = {x0, y0, x1, y1};
  BLResult result = bl_context_set_stroke_width(&painter->context, width);
  if (result != BL_SUCCESS) return result;
  return bl_context_stroke_geometry_rgba32(
      &painter->context, BL_GEOMETRY_TYPE_LINE, &line, rgba32);
}

BLResult retinue_bl_stroke_circle(
    RetinueBlendPainter* painter,
    double center_x,
    double center_y,
    double radius,
    double stroke_width,
    uint32_t rgba32) {
  BLCircle circle = {center_x, center_y, radius};
  BLResult result = bl_context_set_stroke_width(&painter->context, stroke_width);
  if (result != BL_SUCCESS) {
    return result;
  }
  return bl_context_stroke_geometry_rgba32(
      &painter->context, BL_GEOMETRY_TYPE_CIRCLE, &circle, rgba32);
}

BLResult retinue_bl_blit_prgb32(
    RetinueBlendPainter* painter,
    const uint8_t* pixels,
    int source_width,
    int source_height,
    intptr_t source_stride,
    int target_x,
    int target_y,
    int target_width,
    int target_height) {
  BLImageCore image;
  BLResult result = bl_image_init_as_from_data(
      &image,
      source_width,
      source_height,
      BL_FORMAT_PRGB32,
      (void*)pixels,
      source_stride,
      BL_DATA_ACCESS_READ,
      NULL,
      NULL);
  if (result != BL_SUCCESS) {
    return result;
  }

  BLRectI target = {target_x, target_y, target_width, target_height};
  result = bl_context_blit_scaled_image_i(&painter->context, &target, &image, NULL);
  bl_image_reset(&image);
  return result;
}

BLResult retinue_bl_fill_circle_prgb32(
    RetinueBlendPainter* painter,
    const uint8_t* pixels,
    int source_width,
    int source_height,
    intptr_t source_stride,
    int target_x,
    int target_y,
    int target_width,
    int target_height,
    double center_x,
    double center_y,
    double radius) {
  BLImageCore image;
  BLResult result = bl_image_init_as_from_data(
      &image,
      source_width,
      source_height,
      BL_FORMAT_PRGB32,
      (void*)pixels,
      source_stride,
      BL_DATA_ACCESS_READ,
      NULL,
      NULL);
  if (result != BL_SUCCESS) {
    return result;
  }

  BLMatrix2D transform;
  transform.m00 = (double)target_width / (double)source_width;
  transform.m01 = 0.0;
  transform.m10 = 0.0;
  transform.m11 = (double)target_height / (double)source_height;
  transform.m20 = (double)target_x;
  transform.m21 = (double)target_y;

  BLPatternCore pattern;
  result = bl_pattern_init_as(
      &pattern, &image, NULL, BL_EXTEND_MODE_PAD, &transform);
  if (result != BL_SUCCESS) {
    bl_image_reset(&image);
    return result;
  }

  result = bl_context_save(&painter->context, NULL);
  if (result == BL_SUCCESS) {
    BLRectI clip = {target_x, target_y, target_width, target_height};
    result = bl_context_clip_to_rect_i(&painter->context, &clip);
    if (result == BL_SUCCESS) {
      BLCircle circle = {center_x, center_y, radius};
      result = bl_context_fill_geometry_ext(
          &painter->context,
          BL_GEOMETRY_TYPE_CIRCLE,
          &circle,
          (const BLUnknown*)&pattern);
    }
    BLResult restore_result = bl_context_restore(&painter->context, NULL);
    if (result == BL_SUCCESS) {
      result = restore_result;
    }
  }

  bl_pattern_destroy(&pattern);
  bl_image_reset(&image);
  return result;
}

BLResult retinue_bl_fill_a8_mask(
    RetinueBlendPainter* painter,
    const uint8_t* coverage,
    int width,
    int height,
    intptr_t stride,
    int target_x,
    int target_y,
    uint32_t rgba32) {
  if (width == 0 || height == 0) {
    return BL_SUCCESS;
  }

  BLImageCore mask;
  BLResult result = bl_image_init_as_from_data(
      &mask,
      width,
      height,
      BL_FORMAT_A8,
      (void*)coverage,
      stride,
      BL_DATA_ACCESS_READ,
      NULL,
      NULL);
  if (result != BL_SUCCESS) {
    return result;
  }

  BLPointI origin = {target_x, target_y};
  result = bl_context_fill_mask_i_rgba32(
      &painter->context, &origin, &mask, NULL, rgba32);
  bl_image_reset(&mask);
  return result;
}
