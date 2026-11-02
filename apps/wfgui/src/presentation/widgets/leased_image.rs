use crate::domain::media::image_store::ImageHandle;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct LeasedImage {
    pub image: ImageHandle,
}

impl View for LeasedImage {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Image::new(self.image.resource()).resizable()
    }
}
