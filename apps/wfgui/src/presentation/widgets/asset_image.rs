use super::leased_image::LeasedImage;
use crate::{
    data::foundry::Asset,
    domain::media::image_store::{ImageHandle, ImageStore},
};
use retinue_ui::authoring::ViewContext;

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

#[retinue_ui::authoring::view]
impl retinue_ui::authoring::View for AssetImage {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl retinue_ui::authoring::Body {
        use retinue_ui::authoring::{compose, empty, ui};
        ui! {
            if let Some(image) = self.resolve(cx) {
                compose(LeasedImage { image })
            } else {
                empty()
            }
        }
    }
}
