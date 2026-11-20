use crate::{
    resources::ResourceId,
    text::{Blob, Font, FontData, TextError},
};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

pub struct CachedFont {
    id: ResourceId,
    native: Font,
}

#[derive(Default)]
pub struct FontFaces {
    sources: RefCell<HashMap<[u8; 32], Blob<u8>>>,
    faces: RefCell<HashMap<(u64, u32), Font>>,
}

impl CachedFont {
    pub fn id(&self) -> ResourceId {
        self.id
    }
    pub fn native(&self) -> Font {
        self.native.clone()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextError> {
        let native = Font::new(FontData::new(Blob::new(Arc::new(bytes.to_vec())), 0))?;
        Ok(Self {
            id: ResourceId::next(),
            native,
        })
    }
}

impl FontFaces {
    pub fn load(&self, bytes: &[u8], index: u32) -> Result<CachedFont, TextError> {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let mut sources = self.sources.borrow_mut();
        let source = sources
            .get(&digest)
            .cloned()
            .unwrap_or_else(|| Blob::new(Arc::new(bytes.to_vec())));
        let mut faces = self.faces.borrow_mut();
        let key = (source.id(), index);
        let native = match faces.get(&key) {
            Some(font) => font.clone(),
            None => {
                let font = Font::new(FontData::new(source.clone(), index))?;
                sources.entry(digest).or_insert(source);
                faces.insert(key, font.clone());
                font
            }
        };
        Ok(CachedFont {
            id: ResourceId::next(),
            native,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_registry_validates_index_and_reuses_the_same_native_face() {
        let bytes = include_bytes!("../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let faces = FontFaces::default();
        let first = faces.load(bytes, 0).unwrap();
        let second = faces.load(bytes, 0).unwrap();
        assert_eq!(first.native(), second.native());
        assert!(faces.load(bytes, 1).is_err());
        assert!(CachedFont::from_bytes(b"not a font").is_err());
        assert_eq!(faces.faces.borrow().len(), 1);
    }
}
