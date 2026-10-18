use std::ffi::c_int;
use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;

const BL_SUCCESS: u32 = 0;

#[repr(C)]
struct RetinueBlendPainter {
    _private: [u8; 0],
}

struct ImageData {
    width: c_int,
    height: c_int,
    stride: isize,
}

pub(super) struct Image<'a> {
    pub(super) pixels: &'a [u8],
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Rect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Circle {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) radius: f64,
}

pub(super) struct Mask<'a> {
    pub(super) coverage: &'a [u8],
    pub(super) width: usize,
    pub(super) height: usize,
}

unsafe extern "C" {
    fn retinue_bl_painter_create(
        pixels: *mut u8,
        width: c_int,
        height: c_int,
        stride: isize,
        result_out: *mut u32,
    ) -> *mut RetinueBlendPainter;
    fn retinue_bl_painter_destroy(painter: *mut RetinueBlendPainter) -> u32;
    fn retinue_bl_clear(painter: *mut RetinueBlendPainter) -> u32;
    fn retinue_bl_fill_round_rect(
        painter: *mut RetinueBlendPainter,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        rgba32: u32,
    ) -> u32;
    fn retinue_bl_stroke_line(
        painter: *mut RetinueBlendPainter,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        width: f64,
        rgba32: u32,
    ) -> u32;
    fn retinue_bl_stroke_circle(
        painter: *mut RetinueBlendPainter,
        center_x: f64,
        center_y: f64,
        radius: f64,
        stroke_width: f64,
        rgba32: u32,
    ) -> u32;
    fn retinue_bl_blit_prgb32(
        painter: *mut RetinueBlendPainter,
        pixels: *const u8,
        source_width: c_int,
        source_height: c_int,
        source_stride: isize,
        target_x: c_int,
        target_y: c_int,
        target_width: c_int,
        target_height: c_int,
    ) -> u32;
    fn retinue_bl_fill_circle_prgb32(
        painter: *mut RetinueBlendPainter,
        pixels: *const u8,
        source_width: c_int,
        source_height: c_int,
        source_stride: isize,
        target_x: c_int,
        target_y: c_int,
        target_width: c_int,
        target_height: c_int,
        center_x: f64,
        center_y: f64,
        radius: f64,
    ) -> u32;
    fn retinue_bl_fill_a8_mask(
        painter: *mut RetinueBlendPainter,
        coverage: *const u8,
        width: c_int,
        height: c_int,
        stride: isize,
        target_x: c_int,
        target_y: c_int,
        rgba32: u32,
    ) -> u32;
}

pub(super) struct Context<'a> {
    handle: Option<NonNull<RetinueBlendPainter>>,
    width: u32,
    height: u32,
    error: Option<Error>,
    _pixels: PhantomData<&'a mut [u8]>,
}

#[derive(Debug)]
pub struct Error {
    operation: &'static str,
    code: Option<u32>,
}

impl Error {
    fn invalid(operation: &'static str) -> Self {
        Self {
            operation,
            code: None,
        }
    }

    fn blend2d(operation: &'static str, code: u32) -> Self {
        Self {
            operation,
            code: Some(code),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) => write!(
                formatter,
                "Blend2D {} failed with BLResult {code:#010x}",
                self.operation
            ),
            None => write!(formatter, "invalid Blend2D {} arguments", self.operation),
        }
    }
}

impl std::error::Error for Error {}

impl<'a> Context<'a> {
    pub(super) fn new(pixels: &'a mut [u8], width: u32, height: u32) -> Result<Self, Error> {
        let image = image_data(pixels.len(), width as usize, height as usize, 4, "image")?;

        let mut result = BL_SUCCESS;
        let handle = unsafe {
            retinue_bl_painter_create(
                pixels.as_mut_ptr(),
                image.width,
                image.height,
                image.stride,
                &mut result,
            )
        };
        let handle =
            NonNull::new(handle).ok_or_else(|| Error::blend2d("context creation", result))?;
        Ok(Self {
            handle: Some(handle),
            width,
            height,
            error: None,
            _pixels: PhantomData,
        })
    }

    pub(super) fn width(&self) -> u32 {
        self.width
    }

    pub(super) fn height(&self) -> u32 {
        self.height
    }

    pub(super) fn failed(&self) -> bool {
        self.error.is_some()
    }

    pub(super) fn clear_all(&mut self) {
        if self.failed() {
            return;
        }
        let result = unsafe { retinue_bl_clear(self.raw()) };
        self.record("clear", result);
    }

    pub(super) fn fill_round_rect_rgba32(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        rgba32: u32,
    ) {
        if self.failed() {
            return;
        }
        let result =
            unsafe { retinue_bl_fill_round_rect(self.raw(), x, y, width, height, radius, rgba32) };
        self.record("rounded rectangle", result);
    }

    pub(super) fn stroke_line(&mut self, from: (f64, f64), to: (f64, f64), width: f64, color: u32) {
        if self.failed() {
            return;
        }
        let result =
            unsafe { retinue_bl_stroke_line(self.raw(), from.0, from.1, to.0, to.1, width, color) };
        self.record("line stroke", result);
    }

    pub(super) fn stroke_circle_rgba32(
        &mut self,
        center_x: f64,
        center_y: f64,
        radius: f64,
        stroke_width: f64,
        rgba32: u32,
    ) {
        if self.failed() {
            return;
        }
        let result = unsafe {
            retinue_bl_stroke_circle(self.raw(), center_x, center_y, radius, stroke_width, rgba32)
        };
        self.record("circle stroke", result);
    }

    pub(super) fn blit_scaled_prgb32(&mut self, source: Image<'_>, target: Rect) {
        if self.failed() {
            return;
        }
        let Ok(data) = image_data(
            source.pixels.len(),
            source.width as usize,
            source.height as usize,
            4,
            "source image",
        ) else {
            self.record_invalid("source image");
            return;
        };
        let Ok(target_x) = c_int::try_from(target.x) else {
            self.record_invalid("target x");
            return;
        };
        let Ok(target_y) = c_int::try_from(target.y) else {
            self.record_invalid("target y");
            return;
        };
        let Ok(target_width) = c_int::try_from(target.width) else {
            self.record_invalid("target width");
            return;
        };
        let Ok(target_height) = c_int::try_from(target.height) else {
            self.record_invalid("target height");
            return;
        };
        let result = unsafe {
            retinue_bl_blit_prgb32(
                self.raw(),
                source.pixels.as_ptr(),
                data.width,
                data.height,
                data.stride,
                target_x,
                target_y,
                target_width,
                target_height,
            )
        };
        self.record("image blit", result);
    }

    pub(super) fn fill_circle_prgb32(&mut self, source: Image<'_>, target: Rect, circle: Circle) {
        if self.failed() {
            return;
        }
        let Ok(data) = image_data(
            source.pixels.len(),
            source.width as usize,
            source.height as usize,
            4,
            "source image",
        ) else {
            self.record_invalid("source image");
            return;
        };
        let Ok(target_x) = c_int::try_from(target.x) else {
            self.record_invalid("target x");
            return;
        };
        let Ok(target_y) = c_int::try_from(target.y) else {
            self.record_invalid("target y");
            return;
        };
        let Ok(target_width) = c_int::try_from(target.width) else {
            self.record_invalid("target width");
            return;
        };
        let Ok(target_height) = c_int::try_from(target.height) else {
            self.record_invalid("target height");
            return;
        };
        let result = unsafe {
            retinue_bl_fill_circle_prgb32(
                self.raw(),
                source.pixels.as_ptr(),
                data.width,
                data.height,
                data.stride,
                target_x,
                target_y,
                target_width,
                target_height,
                circle.x,
                circle.y,
                circle.radius,
            )
        };
        self.record("circular image fill", result);
    }

    pub(super) fn fill_a8_mask_rgba32(
        &mut self,
        mask: Mask<'_>,
        target_x: i32,
        target_y: i32,
        rgba32: u32,
    ) {
        if self.failed() {
            return;
        }
        if mask.width == 0 || mask.height == 0 {
            return;
        }
        let Ok(data) = image_data(
            mask.coverage.len(),
            mask.width,
            mask.height,
            1,
            "glyph mask",
        ) else {
            self.record_invalid("glyph mask");
            return;
        };
        let result = unsafe {
            retinue_bl_fill_a8_mask(
                self.raw(),
                mask.coverage.as_ptr(),
                data.width,
                data.height,
                data.stride,
                target_x,
                target_y,
                rgba32,
            )
        };
        self.record("glyph mask", result);
    }

    pub(super) fn finish(mut self) -> Result<(), Error> {
        let handle = self.handle.take().expect("Blend2D context handle");
        let finish_result = unsafe { retinue_bl_painter_destroy(handle.as_ptr()) };
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if finish_result != BL_SUCCESS {
            return Err(Error::blend2d("context finish", finish_result));
        }
        Ok(())
    }

    fn raw(&self) -> *mut RetinueBlendPainter {
        self.handle.expect("Blend2D context handle").as_ptr()
    }

    fn record(&mut self, operation: &'static str, result: u32) {
        if result != BL_SUCCESS && self.error.is_none() {
            self.error = Some(Error::blend2d(operation, result));
        }
    }

    fn record_invalid(&mut self, operation: &'static str) {
        if self.error.is_none() {
            self.error = Some(Error::invalid(operation));
        }
    }
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                retinue_bl_painter_destroy(handle.as_ptr());
            }
        }
    }
}

fn image_data(
    buffer_len: usize,
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    operation: &'static str,
) -> Result<ImageData, Error> {
    if width == 0 || height == 0 {
        return Err(Error::invalid(operation));
    }
    let stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Error::invalid(operation))?;
    let required = stride
        .checked_mul(height)
        .ok_or_else(|| Error::invalid(operation))?;
    if buffer_len < required {
        return Err(Error::invalid(operation));
    }
    Ok(ImageData {
        width: c_int::try_from(width).map_err(|_| Error::invalid(operation))?,
        height: c_int::try_from(height).map_err(|_| Error::invalid(operation))?,
        stride: isize::try_from(stride).map_err(|_| Error::invalid(operation))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_rejects_short_target_buffer() {
        let mut pixels = [0; 15];
        assert!(Context::new(&mut pixels, 2, 2).is_err());
    }

    #[test]
    fn context_defers_invalid_source_error_until_finish() {
        let mut target = [0; 16];
        let mut context = Context::new(&mut target, 2, 2).unwrap();
        context.blit_scaled_prgb32(
            Image {
                pixels: &[0; 3],
                width: 1,
                height: 1,
            },
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );

        assert!(context.failed());
        assert!(context.finish().is_err());
    }
}
