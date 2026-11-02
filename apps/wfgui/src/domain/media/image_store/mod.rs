use super::{Asset, Media};
use retinue_render::{ImageSource, thumbnails::Request};
use retinue_ui::image::{ImageRendition, ImageRequest, ImageResource};
use std::{cell::RefCell, collections::HashMap, rc::Rc, rc::Weak};

#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq)]
enum Source {
    Asset(Asset),
    Icon(&'static str),
    Forma,
}

#[derive(Hash, PartialEq, Eq)]
enum SourceKey {
    Asset(String),
    Icon(&'static str),
    Forma,
}

struct Lease {
    source: Source,
    edge: u32,
    embedded: Option<Request>,
    image: ImageResource,
    request: ImageRequest,
}

#[derive(Clone)]
pub(crate) struct ImageHandle(Rc<Lease>);

impl PartialEq for ImageHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ImageHandle {
    pub fn resource(&self) -> ImageResource {
        self.0.image.clone()
    }

    pub fn pixels(&self) -> Option<ImageSource> {
        self.0.image.pixels()
    }
}

impl Lease {
    fn request(&self, media: &Media) -> Option<Request> {
        match &self.source {
            Source::Asset(asset) => media.request(asset, self.edge),
            _ => self.embedded.clone(),
        }
    }

    fn pixels(&self, media: &Media) -> Option<(ImageSource, bool)> {
        match &self.source {
            Source::Asset(asset) => {
                let fresh = media
                    .request(asset, self.edge)
                    .is_some_and(|request| media.embedded_resource(&request).is_some());
                media
                    .image_resource(asset, self.edge)
                    .map(|image| (image.clone().into(), fresh))
            }
            _ => media
                .embedded_resource(self.embedded.as_ref().unwrap())
                .map(|image| (image.clone().into(), true)),
        }
    }
}

#[derive(Default)]
struct Storage {
    leases: HashMap<(SourceKey, u32), Weak<Lease>>,
    published: Option<u64>,
}

#[derive(Clone, Default)]
pub(crate) struct ImageStore(Rc<RefCell<Storage>>);

impl PartialEq for ImageStore {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ImageStore {
    pub fn asset(&self, asset: &Asset, edge: u32) -> ImageHandle {
        let source = if asset.source == "builtin" && asset.image_name == "forma.png" {
            Source::Forma
        } else {
            Source::Asset(asset.clone())
        };
        ImageHandle(self.lease(&source, edge))
    }

    pub fn icon(&self, name: &'static str, edge: u32) -> ImageHandle {
        ImageHandle(self.lease(&Source::Icon(name), edge))
    }

    fn lease(&self, source: &Source, edge: u32) -> Rc<Lease> {
        let key = (
            match source {
                Source::Asset(asset) => SourceKey::Asset(asset.id.clone()),
                Source::Icon(name) => SourceKey::Icon(name),
                Source::Forma => SourceKey::Forma,
            },
            edge,
        );
        let mut storage = self.0.borrow_mut();
        if let Some(lease) = storage.leases.get(&key).and_then(Weak::upgrade) {
            return lease;
        }
        let image = ImageResource::default();
        let lease = Rc::new(Lease {
            source: source.clone(),
            edge,
            embedded: match source {
                Source::Asset(_) => None,
                Source::Icon(name) => Some(Request::embedded(super::icon(name), edge, None)),
                Source::Forma => Some(Request::embedded(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../wfcompanion/assets/forma.png"
                    )),
                    edge,
                    None,
                )),
            },
            request: image.begin(),
            image,
        });
        storage.leases.insert(key, Rc::downgrade(&lease));
        storage.published = None;
        lease
    }

    fn active(&self) -> Vec<Rc<Lease>> {
        let mut leases = Vec::new();
        self.0.borrow_mut().leases.retain(|_, weak| {
            if let Some(lease) = weak.upgrade() {
                leases.push(lease);
                true
            } else {
                false
            }
        });
        leases
    }

    pub fn assets(&self) -> Vec<Asset> {
        self.active()
            .iter()
            .filter_map(|lease| match &lease.source {
                Source::Asset(asset) => Some(asset.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn requests(&self, media: &Media) -> Vec<Request> {
        self.active()
            .iter()
            .filter_map(|lease| lease.request(media))
            .collect()
    }

    pub fn publish(&self, media: &Media) -> bool {
        if self.0.borrow().published == Some(media.revision) {
            return false;
        }
        let mut changed = false;
        for lease in self.active() {
            if let Some((pixels, fresh)) = lease.pixels(media) {
                changed |= lease
                    .image
                    .publish(
                        lease.request,
                        ImageRendition {
                            revision: media.revision,
                            quality: lease.edge,
                            pixels,
                            fresh,
                        },
                    )
                    .expect("current image lease");
                lease
                    .image
                    .complete(lease.request)
                    .expect("current image lease");
            }
        }
        self.0.borrow_mut().published = Some(media.revision);
        changed
    }
}
