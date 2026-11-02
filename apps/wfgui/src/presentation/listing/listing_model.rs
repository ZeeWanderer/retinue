use super::{CardContext, Cell, Metrics};
use retinue_ui::{prelude::*, state::KeyedOrder};

#[derive(Clone, Default, PartialEq)]
pub(super) struct Model {
    pub metrics: State<Metrics>,
    pub weights: State<Vec<f32>>,
    pub mode: State<String>,
    pub context: State<CardContext>,
    pub order: State<KeyedOrder>,
    pub cells: Items<Cell>,
    pub offset: State<f32>,
    pub message: State<(String, bool)>,
}
