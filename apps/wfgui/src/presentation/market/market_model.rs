use super::{AccountState, Metrics, card::Card};
use retinue_ui::prelude::*;

#[derive(Clone, Default, PartialEq)]
pub(super) struct Model {
    pub metrics: State<Metrics>,
    pub account: State<AccountState>,
    pub order: State<KeyedOrder>,
    pub cells: Items<Card>,
    pub offset: State<f32>,
}
