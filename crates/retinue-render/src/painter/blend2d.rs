use std::ffi::{c_int, c_void};
use std::fmt;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use std::num::NonZeroU32;

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

#[repr(C)]
struct NativeImage {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn retinue_bl_image_resample(
        source: *const NativeImage,
        pixels: *mut u8,
        width: c_int,
        height: c_int,
    ) -> u32;
}

#[derive(Debug)]
pub(super) struct Image {
    handle: NonNull<NativeImage>,
    pub(super) width: u32,
    pub(super) height: u32,
    bytes: usize,
    capacity: usize,
    pixels: Arc<Pixels>,
    #[cfg(test)]
    alive: std::sync::Weak<Pixels>,
}

// Sources are immutable; Blend2D retains/releases their data atomically.
unsafe impl Send for Image {}
unsafe impl Sync for Image {}

static IMAGE_COUNT: AtomicUsize = AtomicUsize::new(0);
static PIXEL_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
pub struct ImageMemory {
    pub images: usize,
    pub pixel_bytes: usize,
}

pub fn image_memory() -> ImageMemory {
    ImageMemory {
        images: IMAGE_COUNT.load(Ordering::Relaxed),
        pixel_bytes: PIXEL_BYTES.load(Ordering::Relaxed),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeResources {
    pub jit_used_bytes: usize,
    pub jit_reserved_bytes: usize,
    pub zero_pool_used_bytes: usize,
    pub zero_pool_reserved_bytes: usize,
    pub pipelines: usize,
}

/// Blend2D's global pools, not its total heap or externally owned image pixels.
pub fn runtime_resources() -> Result<RuntimeResources, Error> {
    let mut values = [0; 5];
    let result = unsafe { retinue_bl_runtime_resources(values.as_mut_ptr()) };
    if result != BL_SUCCESS {
        return Err(Error::blend2d("runtime resources", result));
    }
    Ok(RuntimeResources {
        jit_used_bytes: values[0],
        jit_reserved_bytes: values[1],
        zero_pool_used_bytes: values[2],
        zero_pool_reserved_bytes: values[3],
        pipelines: values[4],
    })
}

#[derive(Debug)]
struct Pixels(Vec<u8>);

#[derive(Clone)]
pub(crate) struct ImageStorage {
    pixels: std::sync::Weak<Pixels>,
}

impl ImageStorage {
    pub(crate) fn is_alive(&self) -> bool {
        self.pixels.strong_count() != 0
    }
}

impl Drop for Pixels {
    fn drop(&mut self) {
        IMAGE_COUNT.fetch_sub(1, Ordering::Relaxed);
        PIXEL_BYTES.fetch_sub(self.0.capacity(), Ordering::Relaxed);
    }
}

unsafe extern "C" fn release_pixels(_: *mut c_void, _: *mut c_void, data: *mut c_void) {
    // The native image owns this reference until its last queued use completes.
    drop(unsafe { Arc::from_raw(data.cast::<Pixels>()) });
}

impl Image {
    pub(super) fn resampled(&self, width: u32, height: u32) -> Result<Self, Error> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(Error::invalid("resampled image dimensions"));
        }
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let result = unsafe {
            retinue_bl_image_resample(
                self.handle.as_ptr(),
                pixels.as_mut_ptr(),
                width as c_int,
                height as c_int,
            )
        };
        if result != BL_SUCCESS {
            return Err(Error::blend2d("image resampling", result));
        }
        Self::new(pixels, width, height, false)
    }

    pub(super) fn new(pixels: Vec<u8>, width: u32, height: u32, mask: bool) -> Result<Self, Error> {
        let data = image_data(
            pixels.len(),
            width as usize,
            height as usize,
            if mask { 1 } else { 4 },
            "source image",
        )?;
        let bytes = pixels.len();
        let capacity = pixels.capacity();
        let pixels = Arc::new(Pixels(pixels));
        #[cfg(test)]
        let alive = Arc::downgrade(&pixels);
        IMAGE_COUNT.fetch_add(1, Ordering::Relaxed);
        PIXEL_BYTES.fetch_add(capacity, Ordering::Relaxed);
        let source = pixels.0.as_ptr().cast_mut();
        let owner = Arc::into_raw(pixels.clone());
        let mut result = BL_SUCCESS;
        let handle = unsafe {
            retinue_bl_image_create(
                source,
                data.width,
                data.height,
                data.stride,
                c_int::from(mask),
                release_pixels,
                owner.cast_mut().cast(),
                &mut result,
            )
        };
        let Some(handle) = NonNull::new(handle) else {
            drop(unsafe { Arc::from_raw(owner) });
            return Err(Error::blend2d("source image creation", result));
        };
        Ok(Self {
            handle,
            width,
            height,
            bytes,
            capacity,
            pixels,
            #[cfg(test)]
            alive,
        })
    }

    pub(super) fn byte_len(&self) -> usize {
        self.bytes
    }

    pub(super) fn pixels(&self) -> &[u8] {
        &self.pixels.0
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.capacity + size_of::<Pixels>() + 2 * size_of::<usize>()
    }

    pub(super) fn storage(&self) -> ImageStorage {
        ImageStorage {
            pixels: Arc::downgrade(&self.pixels),
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { retinue_bl_image_destroy(self.handle.as_ptr()) };
    }
}

pub(super) struct Rect {
    pub(super) x: i64,
    pub(super) y: i64,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Circle {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) radius: f64,
}

unsafe extern "C" {
    fn retinue_bl_painter_create(
        pixels: *mut u8,
        width: c_int,
        height: c_int,
        stride: isize,
        thread_count: u32,
        result_out: *mut u32,
    ) -> *mut RetinueBlendPainter;
    fn retinue_bl_painter_destroy(painter: *mut RetinueBlendPainter, errors: *mut u32) -> u32;
    fn retinue_bl_painter_thread_count(painter: *const RetinueBlendPainter) -> u32;
    fn retinue_bl_runtime_resources(values: *mut usize) -> u32;
    fn retinue_bl_image_create(
        pixels: *mut u8,
        width: c_int,
        height: c_int,
        stride: isize,
        mask: c_int,
        destroy: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
        user_data: *mut c_void,
        result_out: *mut u32,
    ) -> *mut NativeImage;
    fn retinue_bl_image_destroy(image: *mut NativeImage);
    fn retinue_bl_clear(painter: *mut RetinueBlendPainter) -> u32;
    #[cfg(test)]
    fn retinue_bl_save(painter: *mut RetinueBlendPainter) -> u32;
    #[cfg(test)]
    fn retinue_bl_restore(painter: *mut RetinueBlendPainter) -> u32;
    #[cfg(test)]
    fn retinue_bl_translate(painter: *mut RetinueBlendPainter, x: f64, y: f64) -> u32;
    fn retinue_bl_set_clip_rect(
        painter: *mut RetinueBlendPainter,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> u32;
    fn retinue_bl_clip_rect(
        painter: *mut RetinueBlendPainter,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> u32;
    fn retinue_bl_fill_round_rect(
        painter: *mut RetinueBlendPainter,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        rgba32: u32,
    ) -> u32;
    fn retinue_bl_stroke_round_rect(
        painter: *mut RetinueBlendPainter,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
        stroke: f64,
        color: u32,
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
    fn retinue_bl_fill_polygon(
        painter: *mut RetinueBlendPainter,
        points: *const [f64; 2],
        count: usize,
        rgba32: u32,
    ) -> u32;
    fn retinue_bl_blit_prgb32(
        painter: *mut RetinueBlendPainter,
        image: *const NativeImage,
        target_x: c_int,
        target_y: c_int,
        target_width: c_int,
        target_height: c_int,
    ) -> u32;
    fn retinue_bl_fill_circle_prgb32(
        painter: *mut RetinueBlendPainter,
        image: *const NativeImage,
        source_width: c_int,
        source_height: c_int,
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
        mask: *const NativeImage,
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
    repair: Option<crate::damage::Region>,
    clip: crate::Rect,
    clips: Vec<crate::Rect>,
    native_clip: crate::Rect,
    _pixels: PhantomData<&'a mut [u8]>,
}

#[derive(Debug)]
pub struct Error {
    operation: &'static str,
    code: Option<u32>,
    deferred: bool,
}

impl Error {
    pub(super) fn invalid(operation: &'static str) -> Self {
        Self {
            operation,
            code: None,
            deferred: false,
        }
    }

    fn blend2d(operation: &'static str, code: u32) -> Self {
        Self {
            operation,
            code: Some(code),
            deferred: false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) if self.deferred => write!(
                formatter,
                "Blend2D deferred rendering failed with flags {code:#010x}"
            ),
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
    pub(super) fn new(
        pixels: &'a mut [u8],
        width: u32,
        height: u32,
        threads: NonZeroU32,
    ) -> Result<Self, Error> {
        let image = image_data(pixels.len(), width as usize, height as usize, 4, "image")?;
        let mut result = BL_SUCCESS;
        let handle = unsafe {
            retinue_bl_painter_create(
                pixels.as_mut_ptr(),
                image.width,
                image.height,
                image.stride,
                threads.get(),
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
            repair: None,
            clip: crate::Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            clips: Vec::new(),
            native_clip: crate::Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
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

    pub(super) fn thread_count(&self) -> u32 {
        unsafe { retinue_bl_painter_thread_count(self.raw()) }
    }

    pub(super) fn fail(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(super) fn clear_all(&mut self) {
        self.draw("clear", None, |raw| unsafe { retinue_bl_clear(raw) });
    }

    pub(super) fn repair(&mut self, region: crate::damage::Region, clip: crate::Rect) {
        if region.rects()
            == [crate::Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            }]
        {
            self.repair = None;
            self.clips.clear();
            self.set_clip_rect(clip);
            return;
        }
        self.repair = Some(region);
        self.clip = clip;
        self.refresh_clips();
    }

    pub(super) fn regions(&self, clip: crate::Rect) -> crate::damage::Region {
        match &self.repair {
            Some(repair) => repair.intersection(self.clip.intersection(clip)),
            None => self.clip.intersection(clip).into(),
        }
    }

    pub(super) fn intersects(&self, rect: [i64; 4]) -> bool {
        let intersects = |r: &crate::Rect| {
            (r.x as i64) < rect[2]
                && (r.y as i64) < rect[3]
                && (r.x as i64 + r.width as i64) > rect[0]
                && (r.y as i64 + r.height as i64) > rect[1]
        };
        if self.repair.is_some() {
            self.clips.iter().any(intersects)
        } else {
            !self.clip.is_empty() && intersects(&self.clip)
        }
    }

    fn refresh_clips(&mut self) {
        self.clips.clear();
        if let Some(repair) = &self.repair {
            self.clips.extend(
                repair
                    .rects()
                    .iter()
                    .map(|r| r.intersection(self.clip))
                    .filter(|r| !r.is_empty()),
            );
        }
    }

    fn draw(
        &mut self,
        operation: &'static str,
        bounds: Option<[f64; 4]>,
        draw: impl Fn(*mut RetinueBlendPainter) -> u32,
    ) {
        if self.failed() {
            return;
        }
        if self.repair.is_none() {
            let result = draw(self.raw());
            self.record(operation, result);
            return;
        }
        for index in 0..self.clips.len() {
            let clip = self.clips[index];
            if self.clips.len() > 1
                && bounds.is_some_and(|b| {
                    clip.x as f64 >= b[2]
                        || clip.y as f64 >= b[3]
                        || (clip.x + clip.width) as f64 <= b[0]
                        || (clip.y + clip.height) as f64 <= b[1]
                })
            {
                continue;
            }
            if self.native_clip != clip {
                let result = unsafe {
                    retinue_bl_set_clip_rect(
                        self.raw(),
                        clip.x as f64,
                        clip.y as f64,
                        clip.width as f64,
                        clip.height as f64,
                    )
                };
                self.record("repair clip", result);
                self.native_clip = clip;
            }
            if self.failed() {
                break;
            }
            let result = draw(self.raw());
            self.record(operation, result);
            if self.failed() {
                break;
            }
        }
    }

    pub(super) fn clip_to_rect(&mut self, rect: crate::Rect) {
        self.clip = self.clip.intersection(rect);
        if self.repair.is_some() {
            self.refresh_clips();
            return;
        }
        if self.failed() {
            return;
        }
        let result = unsafe {
            retinue_bl_clip_rect(
                self.raw(),
                rect.x as f64,
                rect.y as f64,
                rect.width as f64,
                rect.height as f64,
            )
        };
        self.record("clip", result);
        self.native_clip = self.clip;
    }

    pub(super) fn set_clip_rect(&mut self, rect: crate::Rect) {
        self.clip = rect;
        if self.repair.is_some() {
            self.refresh_clips();
            return;
        }
        if self.failed() {
            return;
        }
        let result = unsafe {
            retinue_bl_set_clip_rect(
                self.raw(),
                rect.x as f64,
                rect.y as f64,
                rect.width as f64,
                rect.height as f64,
            )
        };
        self.record("restore clip", result);
        self.native_clip = self.clip;
    }

    #[cfg(test)]
    pub(super) fn save(&mut self) {
        let result = unsafe { retinue_bl_save(self.raw()) };
        self.record("save", result);
    }

    #[cfg(test)]
    pub(super) fn restore(&mut self) {
        let result = unsafe { retinue_bl_restore(self.raw()) };
        self.record("restore", result);
    }

    #[cfg(test)]
    pub(super) fn translate(&mut self, x: f64, y: f64) {
        let result = unsafe { retinue_bl_translate(self.raw(), x, y) };
        self.record("translate", result);
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
        self.draw(
            "rounded rectangle",
            Some([x - 1.0, y - 1.0, x + width + 1.0, y + height + 1.0]),
            |raw| unsafe { retinue_bl_fill_round_rect(raw, x, y, width, height, radius, rgba32) },
        );
    }

    pub(super) fn stroke_line(&mut self, from: (f64, f64), to: (f64, f64), width: f64, color: u32) {
        if self.failed() {
            return;
        }
        let pad = width / 2.0 + 1.0;
        self.draw(
            "line stroke",
            Some([
                from.0.min(to.0) - pad,
                from.1.min(to.1) - pad,
                from.0.max(to.0) + pad,
                from.1.max(to.1) + pad,
            ]),
            |raw| unsafe { retinue_bl_stroke_line(raw, from.0, from.1, to.0, to.1, width, color) },
        );
    }

    pub(super) fn stroke_round_rect(&mut self, rect: Rect, radius: u32, stroke: u32, color: u32) {
        if self.failed() || stroke == 0 || rect.width == 0 || rect.height == 0 {
            return;
        }
        let inset = (stroke as f64 / 2.0).min(rect.width.min(rect.height) as f64 / 2.0);
        let pad = stroke as f64 / 2.0 - inset + 1.0;
        self.draw(
            "rounded rectangle stroke",
            Some([
                rect.x as f64 - pad,
                rect.y as f64 - pad,
                rect.x as f64 + rect.width as f64 + pad,
                rect.y as f64 + rect.height as f64 + pad,
            ]),
            |raw| unsafe {
                retinue_bl_stroke_round_rect(
                    raw,
                    rect.x as f64 + inset,
                    rect.y as f64 + inset,
                    rect.width as f64 - 2.0 * inset,
                    rect.height as f64 - 2.0 * inset,
                    (radius as f64 - inset).max(0.0),
                    stroke as f64,
                    color,
                )
            },
        );
    }

    pub(super) fn fill_polygon(&mut self, points: &[[f64; 2]], color: u32) {
        if self.failed() || points.len() < 3 || points.iter().flatten().any(|v| !v.is_finite()) {
            return;
        }
        let bounds = points.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p[0] - 1.0),
                    b[1].min(p[1] - 1.0),
                    b[2].max(p[0] + 1.0),
                    b[3].max(p[1] + 1.0),
                ]
            },
        );
        self.draw("polygon fill", Some(bounds), |raw| unsafe {
            retinue_bl_fill_polygon(raw, points.as_ptr(), points.len(), color)
        });
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
        let pad = radius + stroke_width / 2.0 + 1.0;
        self.draw(
            "circle stroke",
            Some([
                center_x - pad,
                center_y - pad,
                center_x + pad,
                center_y + pad,
            ]),
            |raw| unsafe {
                retinue_bl_stroke_circle(raw, center_x, center_y, radius, stroke_width, rgba32)
            },
        );
    }

    pub(super) fn blit_scaled_prgb32(&mut self, source: &Image, target: Rect) {
        if self.failed() {
            return;
        }
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
        self.draw(
            "image blit",
            Some([
                target_x as f64,
                target_y as f64,
                target_x as f64 + target_width as f64,
                target_y as f64 + target_height as f64,
            ]),
            |raw| unsafe {
                retinue_bl_blit_prgb32(
                    raw,
                    source.handle.as_ptr(),
                    target_x,
                    target_y,
                    target_width,
                    target_height,
                )
            },
        );
    }

    pub(super) fn fill_circle_prgb32(&mut self, source: &Image, target: Rect, circle: Circle) {
        if self.failed() {
            return;
        }
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
        self.draw(
            "circular image fill",
            Some([
                target_x as f64 - 1.0,
                target_y as f64 - 1.0,
                target_x as f64 + target_width as f64 + 1.0,
                target_y as f64 + target_height as f64 + 1.0,
            ]),
            |raw| unsafe {
                retinue_bl_fill_circle_prgb32(
                    raw,
                    source.handle.as_ptr(),
                    source.width as c_int,
                    source.height as c_int,
                    target_x,
                    target_y,
                    target_width,
                    target_height,
                    circle.x,
                    circle.y,
                    circle.radius,
                )
            },
        );
    }

    pub(super) fn fill_a8_mask_rgba32(
        &mut self,
        mask: &Image,
        target_x: i64,
        target_y: i64,
        rgba32: u32,
    ) {
        if self.failed() {
            return;
        }
        let (Ok(target_x), Ok(target_y)) = (c_int::try_from(target_x), c_int::try_from(target_y))
        else {
            self.record_invalid("mask origin");
            return;
        };
        self.draw(
            "glyph mask",
            Some([
                target_x as f64,
                target_y as f64,
                target_x as f64 + mask.width as f64,
                target_y as f64 + mask.height as f64,
            ]),
            |raw| unsafe {
                retinue_bl_fill_a8_mask(raw, mask.handle.as_ptr(), target_x, target_y, rgba32)
            },
        );
    }

    pub(super) fn finish(mut self) -> Result<(), Error> {
        let handle = self.handle.take().expect("Blend2D context handle");
        let mut deferred = 0;
        let finish_result = unsafe { retinue_bl_painter_destroy(handle.as_ptr(), &mut deferred) };
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        if finish_result != BL_SUCCESS {
            return Err(Error::blend2d("context finish", finish_result));
        }
        if deferred != 0 {
            return Err(Error {
                operation: "rendering",
                code: Some(deferred),
                deferred: true,
            });
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
                retinue_bl_painter_destroy(handle.as_ptr(), &mut 0);
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
    fn queued_polygon_consumes_points_before_the_input_is_reused() {
        for threads in [1, 2, 4] {
            let mut pixels = vec![0; 32 * 32 * 4];
            let mut context =
                Context::new(&mut pixels, 32, 32, NonZeroU32::new(threads).unwrap()).unwrap();
            let mut points = vec![[4.0, 4.0], [28.0, 4.0], [28.0, 28.0], [4.0, 28.0]];
            context.fill_polygon(&points, 0xff336699);
            points.fill([0.0, 0.0]);
            drop(points);
            context.finish().unwrap();
            assert_eq!(
                &pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4],
                &[0x99, 0x66, 0x33, 255]
            );
            assert_eq!(&pixels[..4], &[0; 4]);
        }
    }

    #[test]
    fn bridge_rejects_synchronous_contexts() {
        let mut pixels = [0; 4];
        let mut result = BL_SUCCESS;
        let handle =
            unsafe { retinue_bl_painter_create(pixels.as_mut_ptr(), 1, 1, 4, 0, &mut result) };
        if !handle.is_null() {
            unsafe { retinue_bl_painter_destroy(handle, std::ptr::null_mut()) };
        }
        assert!(handle.is_null());
        assert_ne!(result, BL_SUCCESS);
    }

    #[test]
    fn context_rejects_short_target_buffer() {
        let mut pixels = [0; 15];
        assert!(Context::new(&mut pixels, 2, 2, NonZeroU32::MIN).is_err());
    }

    #[test]
    fn sources_reject_invalid_storage_before_submission() {
        assert!(Image::new(vec![0; 3], 1, 1, false).is_err());
        assert!(Image::new(vec![], 0, 1, true).is_err());
        assert!(Image::new(vec![0; 4], u32::MAX, 1, true).is_err());
    }

    #[test]
    fn resampled_image_does_not_retain_original_pixels() {
        let original = Image::new(vec![255; 512 * 512 * 4], 512, 512, false).unwrap();
        let alive = original.alive.clone();
        let thumbnail = original.resampled(32, 32).unwrap();
        drop(original);
        assert!(alive.upgrade().is_none());
        assert_eq!(thumbnail.byte_len(), 32 * 32 * 4);
        assert!(thumbnail.pixels().iter().all(|byte| *byte == 255));
    }

    #[test]
    fn context_defers_invalid_target_error_until_finish() {
        let mut target = [0; 16];
        let mut context = Context::new(&mut target, 2, 2, NonZeroU32::MIN).unwrap();
        let source = Image::new(vec![0; 4], 1, 1, false).unwrap();
        context.blit_scaled_prgb32(
            &source,
            Rect {
                x: i64::from(u32::MAX),
                y: 0,
                width: 1,
                height: 1,
            },
        );

        assert!(context.failed());
        assert!(context.finish().is_err());
    }

    #[test]
    fn queued_images_and_masks_live_until_completion_or_drop() {
        for threads in [1, 2, 4] {
            for finish in [false, true] {
                let mut pixels = vec![0; 64 * 64 * 4];
                let mut ctx =
                    Context::new(&mut pixels, 64, 64, NonZeroU32::new(threads).unwrap()).unwrap();
                assert!(ctx.thread_count() >= 1);
                let source = Image::new(vec![255; 4 * 4 * 4], 4, 4, false).unwrap();
                let mask = Image::new(vec![128; 4 * 4], 4, 4, true).unwrap();
                let (source_alive, mask_alive) = (source.alive.clone(), mask.alive.clone());
                ctx.blit_scaled_prgb32(
                    &source,
                    Rect {
                        x: 0,
                        y: 0,
                        width: 4,
                        height: 4,
                    },
                );
                ctx.fill_a8_mask_rgba32(&mask, 8, 8, 0xFFFFFFFF);
                drop((source, mask));
                assert!(source_alive.upgrade().is_some());
                assert!(mask_alive.upgrade().is_some());
                if finish {
                    ctx.finish().unwrap();
                } else {
                    drop(ctx);
                }
                assert!(source_alive.upgrade().is_none());
                assert!(mask_alive.upgrade().is_none());
                assert_eq!(&pixels[..4], &[255; 4]);
                assert_eq!(pixels[(8 * 64 + 8) * 4 + 3], 128);
            }
        }
    }

    #[test]
    fn failed_frame_still_finishes_queued_sources() {
        let mut pixels = vec![0; 64 * 64 * 4];
        let mut ctx = Context::new(&mut pixels, 64, 64, NonZeroU32::new(2).unwrap()).unwrap();
        let source = Image::new(vec![255; 16], 2, 2, false).unwrap();
        let alive = source.alive.clone();
        ctx.blit_scaled_prgb32(
            &source,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        );
        drop(source);
        ctx.record_invalid("later operation");
        assert!(ctx.finish().is_err());
        assert!(alive.upgrade().is_none());
    }
}
