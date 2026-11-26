//! Templates and typed instances upstream of layout and draw IR.
//!
//! [`Template`] shares static structure and maps comparable input sources to
//! property bindings. [`InstanceGraph`] stages typed inputs and dynamic child
//! order without rebuilding the authored view tree. It owns callbacks on the
//! caller thread; [`crate::scene::LayoutExecution`] may execute numeric layout
//! work separately.
//!
//! Logical structure does not require a layout box per owner. [`Kind::Group`]
//! flattens static siblings and [`Kind::Slot`] splices dynamic roots; either can
//! expose zero or multiple roots. They have identities but no geometry of their
//! own. Layout and paint properties target materialized nodes.
//!
//! [`InstanceGraph::observe`] binds a typed input record to tracked model and
//! environment reads. Only dirty readers run on the owner thread. This is an
//! input binding; [`crate::authoring`] compiles composite view bodies. Contextual inputs
//! use [`InstanceGraph::observe_in`] for retained state and deadline-driven sampling.
//! The host schedules [`InstanceGraph::next_deadline`]; idle owners are not polled.
//! [`Property::Input`] publishes typed event bindings with their geometry, including
//! owners spanning multiple roots. [`InstanceGraph::dispatch`] routes against that
//! publication while future layout runs; handler-only edits do not relayout.
//!
//! Acceptance and publication are distinct. [`InstanceGraph::advance`] admits
//! staged inputs when the backend is ready; [`Update::completion`] may still be
//! pending or fail. Published geometry and outgoing owner state survive until a
//! replacement publishes. Calls while work is pending leave new inputs staged.

#[cfg(test)]
mod backend_tests;
#[cfg(test)]
mod benchmark;
#[cfg(test)]
mod collection_benchmark;
#[cfg(test)]
mod collection_tests;
mod edits;
#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod fragment_tests;
mod graph;
#[cfg(test)]
mod graph_tests;
#[cfg(test)]
mod input_benchmark;
#[cfg(test)]
mod input_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod observation_benchmark;
mod owner;
#[cfg(test)]
mod property_tests;
mod store;
mod structure;
mod template;
#[cfg(test)]
mod tests;

pub use graph::{Address, Child, InstanceGraph, Update};
use store::Bank;
pub use store::Handle;
pub use structure::{Declaration, Kind, Local, Structure};
pub(crate) use template::SourceGroup;
pub use template::{Attributes, Binding, Property, Recipe, Source, Template, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub owners_evaluated: usize,
    pub invalid_deadlines: usize,
    pub input_updates: usize,
    pub sources_compared: usize,
    pub bindings_emitted: usize,
    pub instances_changed: usize,
    pub instances_mounted: usize,
    pub instances_unmounted: usize,
    pub nodes_materialized: usize,
    #[cfg(feature = "profiling")]
    pub existing_nodes_materialized: usize,
    pub topology_visited: usize,
}

pub type Result<T> = std::result::Result<T, &'static str>;
