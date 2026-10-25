#include <blend2d.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct RetinueBlendPainter {
  BLImageCore image;
  BLContextCore context;
} RetinueBlendPainter;

RetinueBlendPainter* retinue_bl_painter_create(
    uint8_t* pixels,
    int width,
    int height,
    intptr_t stride,
    uint32_t thread_count,
    BLResult* result_out) {
  RetinueBlendPainter* painter = NULL;
  BLResult result = BL_SUCCESS;

  if (!pixels || width <= 0 || height <= 0 || stride < (intptr_t)width * 4 || !thread_count) {
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

  BLContextCreateInfo options = {0};
  options.thread_count = thread_count;
  result = bl_context_init_as(&painter->context, &painter->image, &options);
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

BLResult retinue_bl_painter_destroy(RetinueBlendPainter* painter, uint32_t* errors_out) {
  *errors_out = 0;
  if (!painter) {
    return BL_SUCCESS;
  }
  BLResult result = bl_context_flush(&painter->context, BL_CONTEXT_FLUSH_SYNC);
  BLResult property_result = bl_object_get_property_uint32(
      &painter->context, "accumulated_error_flags", SIZE_MAX, errors_out);
  BLResult end_result = bl_context_end(&painter->context);
  bl_image_reset(&painter->image);
  free(painter);
  return result != BL_SUCCESS ? result :
      property_result != BL_SUCCESS ? property_result : end_result;
}

uint32_t retinue_bl_painter_thread_count(const RetinueBlendPainter* painter) {
  uint32_t count = 0;
  bl_object_get_property_uint32(&painter->context, "thread_count", SIZE_MAX, &count);
  return count;
}

BLImageCore* retinue_bl_image_create(
    uint8_t* pixels, int width, int height, intptr_t stride, int mask,
    BLDestroyExternalDataFunc destroy, void* user_data, BLResult* result_out) {
  BLImageCore* image = malloc(sizeof(BLImageCore));
  if (!image) {
    *result_out = BL_ERROR_OUT_OF_MEMORY;
    return NULL;
  }
  *result_out = bl_image_init_as_from_data(
      image, width, height, mask ? BL_FORMAT_A8 : BL_FORMAT_PRGB32,
      pixels, stride, BL_DATA_ACCESS_READ, destroy, user_data);
  if (*result_out != BL_SUCCESS) {
    free(image);
    return NULL;
  }
  return image;
}

void retinue_bl_image_destroy(BLImageCore* image) {
  bl_image_destroy(image);
  free(image);
}

BLResult retinue_bl_image_resample(const BLImageCore* source, uint8_t* pixels, int width, int height) {
  BLImageCore scaled;
  bl_image_init(&scaled);
  BLSizeI size = {width, height};
  BLResult result = bl_image_scale(&scaled, source, &size, BL_IMAGE_SCALE_FILTER_LANCZOS);
  if (result == BL_SUCCESS) {
    BLImageData data;
    result = bl_image_get_data(&scaled, &data);
    if (result == BL_SUCCESS) {
      for (int y = 0; y < height; ++y) {
        memcpy(pixels + (size_t)y * width * 4,
               (const uint8_t*)data.pixel_data + y * data.stride, (size_t)width * 4);
      }
    }
  }
  bl_image_destroy(&scaled);
  return result;
}

BLResult retinue_bl_clear(RetinueBlendPainter* painter) {
  return bl_context_clear_all(&painter->context);
}

BLResult retinue_bl_clip_rect(RetinueBlendPainter* painter, double x, double y, double width, double height) {
  BLRect rect = {x, y, width, height};
  return bl_context_clip_to_rect_d(&painter->context, &rect);
}

BLResult retinue_bl_set_clip_rect(RetinueBlendPainter* painter, double x, double y, double width, double height) {
  BLResult result = bl_context_restore_clipping(&painter->context);
  if (result != BL_SUCCESS) return result;
  return retinue_bl_clip_rect(painter, x, y, width, height);
}

BLResult retinue_bl_save(RetinueBlendPainter* painter) {
  return bl_context_save(&painter->context, NULL);
}

BLResult retinue_bl_restore(RetinueBlendPainter* painter) {
  return bl_context_restore(&painter->context, NULL);
}

BLResult retinue_bl_translate(RetinueBlendPainter* painter, double x, double y) {
  BLPoint point = {x, y};
  return bl_context_apply_transform_op(&painter->context, BL_TRANSFORM_OP_TRANSLATE, &point);
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

BLResult retinue_bl_stroke_round_rect(RetinueBlendPainter* painter, double x, double y,
    double width, double height, double radius, double stroke, uint32_t rgba32) {
  BLRoundRect rect = {x, y, width, height, radius, radius};
  BLResult result = bl_context_set_stroke_width(&painter->context, stroke);
  if (result != BL_SUCCESS) return result;
  return bl_context_stroke_geometry_rgba32(&painter->context, BL_GEOMETRY_TYPE_ROUND_RECT, &rect, rgba32);
}

BLResult retinue_bl_stroke_line(RetinueBlendPainter* painter, double x0, double y0,
                         double x1, double y1, double width, uint32_t rgba32) {
  BLLine line = {x0, y0, x1, y1};
  BLResult result = bl_context_set_stroke_width(&painter->context, width);
  if (result != BL_SUCCESS) return result;
  return bl_context_stroke_geometry_rgba32(
      &painter->context, BL_GEOMETRY_TYPE_LINE, &line, rgba32);
}

BLResult retinue_bl_fill_polygon(RetinueBlendPainter* painter, const BLPoint* points,
                                size_t count, uint32_t rgba32) {
  BLArrayView polygon = {points, count};
  return bl_context_fill_geometry_rgba32(
      &painter->context, BL_GEOMETRY_TYPE_POLYGOND, &polygon, rgba32);
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
    const BLImageCore* image,
    int target_x,
    int target_y,
    int target_width,
    int target_height) {
  BLRectI target = {target_x, target_y, target_width, target_height};
  return bl_context_blit_scaled_image_i(&painter->context, &target, image, NULL);
}

BLResult retinue_bl_fill_circle_prgb32(
    RetinueBlendPainter* painter,
    const BLImageCore* image,
    int source_width,
    int source_height,
    int target_x,
    int target_y,
    int target_width,
    int target_height,
    double center_x,
    double center_y,
    double radius) {
  BLMatrix2D transform;
  transform.m00 = (double)target_width / (double)source_width;
  transform.m01 = 0.0;
  transform.m10 = 0.0;
  transform.m11 = (double)target_height / (double)source_height;
  transform.m20 = (double)target_x;
  transform.m21 = (double)target_y;

  BLPatternCore pattern;
  BLResult result = bl_pattern_init_as(
      &pattern, image, NULL, BL_EXTEND_MODE_PAD, &transform);
  if (result != BL_SUCCESS) {
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
  return result;
}

BLResult retinue_bl_fill_a8_mask(
    RetinueBlendPainter* painter,
    const BLImageCore* mask,
    int target_x,
    int target_y,
    uint32_t rgba32) {
  BLPointI origin = {target_x, target_y};
  return bl_context_fill_mask_i_rgba32(
      &painter->context, &origin, mask, NULL, rgba32);
}
