use super::{Context, Error, RetinueBlendPainter};
use crate::text::{Blob, Font, Glyph, GlyphRun};
use std::{ffi::c_void, ptr::NonNull};

#[repr(C)]
struct NativeFont {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn retinue_bl_font_create(
        bytes: *const u8,
        len: usize,
        index: u32,
        size: f32,
        destroy: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
        data: *mut c_void,
        result: *mut u32,
    ) -> *mut NativeFont;
    fn retinue_bl_font_destroy(font: *mut NativeFont);
    fn retinue_bl_fill_glyph_run(
        painter: *mut RetinueBlendPainter,
        font: *const NativeFont,
        glyphs: *const Glyph,
        count: usize,
        x: f64,
        y: f64,
        scale: f64,
        color: u32,
    ) -> u32;
}

/// Immutable native face/size, reusable across runs and submissions.
/// The native data lease survives this handle when a queued batch still needs it.
pub struct PreparedFont {
    handle: NonNull<NativeFont>,
    identity: (u64, u32),
    size: f32,
}

// No font mutators escape this wrapper. Glyph submission only reads BLFontCore;
// Blend2D's RenderJob_TextOp retains the shared implementation atomically.
unsafe impl Send for PreparedFont {}
unsafe impl Sync for PreparedFont {}

impl PreparedFont {
    pub fn new(font: &Font, size: f32) -> Result<Self, Error> {
        if !size.is_finite() || size <= 0.0 {
            return Err(Error::invalid("font size"));
        }
        let data = font.data();
        let lease = Box::into_raw(Box::new(data.data.clone()));
        let mut result = 0;
        // The bridge consumes the lease on every path, including creation failure.
        let handle = unsafe {
            retinue_bl_font_create(
                data.data.data().as_ptr(),
                data.data.len(),
                data.index,
                size,
                release_font_data,
                lease.cast(),
                &mut result,
            )
        };
        Ok(Self {
            handle: NonNull::new(handle).ok_or_else(|| Error::blend2d("font creation", result))?,
            identity: font.identity(),
            size,
        })
    }
}

unsafe extern "C" fn release_font_data(_: *mut c_void, _: *mut c_void, data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Blob<u8>>()) });
}

impl Drop for PreparedFont {
    fn drop(&mut self) {
        unsafe { retinue_bl_font_destroy(self.handle.as_ptr()) };
    }
}

impl Context<'_> {
    pub(in crate::painter) fn fill_glyph_run(
        &mut self,
        font: &PreparedFont,
        run: &GlyphRun,
        origin: (f64, f64),
        scale: f64,
        color: u32,
    ) {
        if font.identity != run.font().identity()
            || font.size != run.size()
            || !scale.is_finite()
            || scale <= 0.0
            || !origin.0.is_finite()
            || !origin.1.is_finite()
        {
            self.fail(Error::invalid("glyph run font or transform"));
            return;
        }
        if run.glyphs().is_empty() {
            return;
        }
        let ink = run.ink_bounds();
        let bounds = [
            origin.0 + ink.x as f64 * scale - 1.0,
            origin.1 + ink.y as f64 * scale - 1.0,
            origin.0 + (ink.x + ink.width) as f64 * scale + 1.0,
            origin.1 + (ink.y + ink.height) as f64 * scale + 1.0,
        ];
        if !bounds
            .into_iter()
            .chain([origin.0 / scale, origin.1 / scale])
            .all(f64::is_finite)
        {
            self.fail(Error::invalid("glyph run transformed bounds"));
            return;
        }
        self.draw("glyph run", Some(bounds), |raw| unsafe {
            retinue_bl_fill_glyph_run(
                raw,
                font.handle.as_ptr(),
                run.glyphs().as_ptr(),
                run.glyphs().len(),
                origin.0,
                origin.1,
                scale,
                color,
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn font_creation_failures_release_the_external_data_once() {
        let font = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        for (bytes, index) in [(b"not a font".as_slice(), 0), (font.as_slice(), 999)] {
            let blob = Blob::new(Arc::new(bytes.to_vec()));
            let weak = blob.downgrade();
            let pointer = blob.data().as_ptr();
            let len = blob.len();
            let lease = Box::into_raw(Box::new(blob));
            let mut result = 0;
            let handle = unsafe {
                retinue_bl_font_create(
                    pointer,
                    len,
                    index,
                    16.0,
                    release_font_data,
                    lease.cast(),
                    &mut result,
                )
            };
            assert!(handle.is_null());
            assert_ne!(result, 0);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn shared_font_survives_concurrent_native_passes_and_releases_source() {
        use crate::{
            Color,
            painter::RasterImage,
            text::{FontData, Hinting},
            work::{Executor, Graph, Limits, Priority},
        };
        use std::{
            num::{NonZeroU32, NonZeroUsize},
            sync::mpsc,
            time::Duration,
        };

        let blob = Blob::new(Arc::new(
            include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf").to_vec(),
        ));
        let weak = blob.downgrade();
        let face = Font::new(FontData::new(blob, 0)).unwrap();
        let font = Arc::new(PreparedFont::new(&face, 18.0).unwrap());
        let run = GlyphRun::new(
            face,
            18.0,
            vec![
                Glyph {
                    id: 42,
                    x: 2.25,
                    y: 24.5,
                },
                Glyph {
                    id: 68,
                    x: 17.5,
                    y: 24.5,
                },
            ],
            Hinting::None,
        )
        .unwrap();
        let paint = |font: &PreparedFont, run: &GlyphRun, threads| {
            RasterImage::render_with_threads(80, 40, threads, |painter| {
                painter.draw_glyph_run(font, run, (1.0, 0.0), 1.0, Color::rgb(0xffffff));
            })
            .unwrap()
        };
        let expected = paint(&font, &run, NonZeroU32::MIN).to_rgba().unwrap();
        let executor = Executor::new(Limits {
            participants: NonZeroUsize::new(5).unwrap(),
            tasks: 16,
        });
        let pause = executor.pause_background();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let mut graph = Graph::default();
        let images: Vec<_> = (0..12)
            .map(|_| {
                let (font, run) = (font.clone(), run.clone());
                let barrier = barrier.clone();
                graph.add(&[], NonZeroUsize::new(2).unwrap(), move |_| {
                    barrier.wait();
                    Ok(paint(&font, &run, NonZeroU32::new(2).unwrap()))
                })
            })
            .collect();
        let (send, receive) = mpsc::channel();
        let submitted = executor
            .submit(graph, Priority::Background, move || {
                let _ = send.send(());
            })
            .unwrap();
        drop(font);
        drop(run);
        assert!(weak.upgrade().is_some());
        drop(pause);
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(submitted.try_complete().unwrap().completed, images.len());
        for image in images {
            assert_eq!(image.read().unwrap().unwrap().to_rgba().unwrap(), expected);
        }
        assert!(weak.upgrade().is_none());
    }
}
