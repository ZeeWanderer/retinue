use super::leased_image::LeasedImage;
use crate::{
    data::foundry::Asset,
    domain::media::image_store::{ImageHandle, ImageStore},
};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
enum Source {
    Asset(Asset),
    Icon(&'static str),
}

#[derive(PartialEq)]
pub(crate) struct AssetImage {
    source: Option<Source>,
    edge: u32,
}

impl AssetImage {
    pub fn new(asset: Option<&Asset>, edge: u32) -> Self {
        Self {
            source: asset.cloned().map(Source::Asset),
            edge,
        }
    }

    pub fn icon(name: &'static str, edge: u32) -> Self {
        Self {
            source: Some(Source::Icon(name)),
            edge,
        }
    }

    pub fn resolve(&self, cx: &mut ViewContext<'_>) -> Option<ImageHandle> {
        let store = cx
            .environment()
            .model::<ImageStore>()
            .expect("page image host");
        self.source.as_ref().map(|source| match source {
            Source::Asset(asset) => store.asset(asset, self.edge),
            Source::Icon(name) => store.icon(name, self.edge),
        })
    }
}

impl View for AssetImage {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            if let Some(image) = self.resolve(cx) {
                LeasedImage { image }
            } else {
                Canvas::new(retinue_ui::drawing::DrawList::default())
            }
        }
    }
}
