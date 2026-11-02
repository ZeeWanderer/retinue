//! UI-thread publication of immutable renditions. Acquisition remains host-owned.

use crate::{
    layout::LayoutSize,
    state::{State, untracked},
};
use retinue_render::ImageSource;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(u64);

impl ImageId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageStatus {
    Idle,
    Loading,
    Ready,
    Failed(Rc<str>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRequest {
    source: ImageId,
    serial: u64,
}

pub struct ImageRendition {
    pub revision: u64,
    pub quality: u32,
    pub pixels: ImageSource,
    pub fresh: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageUpdateError {
    ObsoleteRequest,
    InvalidMetadata,
}

#[derive(Clone)]
pub struct ImageResource(Rc<Storage>);

struct Storage {
    id: ImageId,
    serial: Cell<u64>,
    accepted: RefCell<Option<(u64, u32)>>,
    metadata: State<Option<LayoutSize>>,
    pixels: State<Option<ImageSource>>,
    fresh: State<bool>,
    status: State<ImageStatus>,
}

impl Default for ImageResource {
    fn default() -> Self {
        Self(Rc::new(Storage {
            id: ImageId::next(),
            serial: Cell::new(0),
            accepted: RefCell::new(None),
            metadata: State::new(None),
            pixels: State::new(None),
            fresh: State::new(false),
            status: State::new(ImageStatus::Idle),
        }))
    }
}

impl PartialEq for ImageResource {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ImageResource {}

impl ImageResource {
    pub fn ready(pixels: impl Into<ImageSource>) -> Self {
        let resource = Self::default();
        let request = resource.begin();
        resource
            .publish(
                request,
                ImageRendition {
                    revision: 0,
                    quality: u32::MAX,
                    pixels: pixels.into(),
                    fresh: true,
                },
            )
            .unwrap();
        resource.complete(request).unwrap();
        resource
    }

    pub fn id(&self) -> ImageId {
        self.0.id
    }
    pub fn metadata(&self) -> Option<LayoutSize> {
        self.0.metadata.get()
    }
    pub fn pixels(&self) -> Option<ImageSource> {
        self.0.pixels.get()
    }
    pub fn status(&self) -> ImageStatus {
        self.0.status.get()
    }
    pub fn is_fresh(&self) -> bool {
        self.0.fresh.get()
    }

    pub fn begin(&self) -> ImageRequest {
        let serial = self
            .0
            .serial
            .get()
            .checked_add(1)
            .expect("image request identity exhausted");
        self.0.serial.set(serial);
        self.0.fresh.set(false);
        self.0.status.set(ImageStatus::Loading);
        ImageRequest {
            source: self.0.id,
            serial,
        }
    }

    fn validate(&self, request: ImageRequest) -> Result<(), ImageUpdateError> {
        if request.source != self.0.id || request.serial != self.0.serial.get() {
            Err(ImageUpdateError::ObsoleteRequest)
        } else {
            Ok(())
        }
    }

    pub fn set_metadata(
        &self,
        request: ImageRequest,
        size: LayoutSize,
    ) -> Result<bool, ImageUpdateError> {
        self.validate(request)?;
        if !size.valid() || size.width <= 0.0 || size.height <= 0.0 {
            return Err(ImageUpdateError::InvalidMetadata);
        }
        Ok(self.0.metadata.set(Some(size)))
    }

    pub fn publish(
        &self,
        request: ImageRequest,
        rendition: ImageRendition,
    ) -> Result<bool, ImageUpdateError> {
        self.validate(request)?;
        let version = (rendition.revision, rendition.quality);
        if self
            .0
            .accepted
            .borrow()
            .is_some_and(|accepted| version < accepted)
        {
            return Ok(false);
        }
        *self.0.accepted.borrow_mut() = Some(version);
        let dimensions = rendition.pixels.dimensions();
        if untracked(|| self.0.metadata.get()).is_none() {
            self.set_metadata(
                request,
                LayoutSize::new(dimensions.0 as f32, dimensions.1 as f32),
            )?;
        }
        let changed = self.0.pixels.set(Some(rendition.pixels));
        self.0.fresh.set(rendition.fresh);
        Ok(changed)
    }

    pub fn complete(&self, request: ImageRequest) -> Result<(), ImageUpdateError> {
        self.validate(request)?;
        self.0.status.set(ImageStatus::Ready);
        Ok(())
    }

    pub fn fail(
        &self,
        request: ImageRequest,
        message: impl Into<Rc<str>>,
    ) -> Result<(), ImageUpdateError> {
        self.validate(request)?;
        self.0.status.set(ImageStatus::Failed(message.into()));
        Ok(())
    }
}
