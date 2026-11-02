use super::ViewId;
use crate::{
    environment::Environment,
    state::{Mutation, Observation},
    view::{Element, Owned},
};

pub(super) struct Node {
    pub parent: Option<ViewId>,
    pub depth: usize,
    pub input: Element,
    pub applied: Option<Element>,
    pub children: Vec<ViewId>,
    pub environment: Environment,
    pub body: Option<Body>,
    pub primitive: Option<crate::view::PrimitiveOutput>,
    pub revision: u64,
    pub cause: Option<Mutation>,
    pub last_mutation: Option<Mutation>,
    pub retry_body: bool,
    pub timed_update: bool,
    pub animation_scope: Option<(u64, crate::state::Transaction)>,
}

pub(super) struct Body {
    pub owned: Owned,
    pub observation: Observation,
    pub deadlines: crate::timeline::Deadlines,
}
