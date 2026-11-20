use crate::{Key, input::InputOptions, scene};

#[derive(Clone, PartialEq)]
pub(super) struct Input {
    pub enabled: bool,
    pub options: InputOptions,
}

#[derive(Default)]
pub(super) struct Edits {
    pub scene: Vec<scene::Change>,
    pub inputs: Vec<(Key, Option<Input>)>,
    pub names: Vec<(Key, Option<Key>)>,
}

impl Edits {
    pub fn clear(&mut self) {
        self.scene.clear();
        self.inputs.clear();
        self.names.clear();
    }
}
