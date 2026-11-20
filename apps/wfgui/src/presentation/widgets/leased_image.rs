use crate::domain::media::image_store::ImageHandle;
use retinue_ui::authoring::{Image, ViewContext};

#[derive(PartialEq)]
pub(super) struct LeasedImage {
    pub image: ImageHandle,
}

#[retinue_ui::authoring::view]
impl retinue_ui::authoring::View for LeasedImage {
    fn body(&self, _: &mut ViewContext<'_>) -> impl retinue_ui::authoring::Body {
        Image::new(self.image.resource()).resizable()
    }
}
