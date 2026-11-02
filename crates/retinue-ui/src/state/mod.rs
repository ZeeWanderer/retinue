//! UI-thread observable data, independent of layout and rendering.
//!
//! `#[observable]` generates model ownership, leaving value access explicit:
//! ```
//! use retinue_ui::{observable, state::Observation};
//! #[observable]
//! struct Item { name: String, rank: u32 }
//! let item = Item::new("Boar".into(), 0);
//! let mut observation = Observation::new(|| {});
//! observation.track(|| item.rank.get());
//! item.rank.set(1);
//! assert!(observation.is_dirty());
//! assert_eq!(&*item.name.read(), "Boar");
//! ```
//!
//! Borrowing works without Clone; `get` explicitly requests a copy:
//! ```compile_fail
//! use retinue_ui::observable;
//! struct Payload;
//! #[observable]
//! struct Model { value: Payload }
//! let model = Model::new(Payload);
//! let copy = model.value.get();
//! ```
//!
//! Properties retain their identity. Change their values, not the property handles:
//! ```compile_fail
//! use retinue_ui::{observable, state::State};
//! #[observable]
//! struct Model { rank: u32 }
//! let mut model = Model::new(0);
//! model.rank = State::new(10);
//! ```
//!
//! Field visibility remains the declaration's visibility:
//! ```compile_fail
//! mod model {
//!     #[retinue_ui::observable]
//!     pub struct Model { secret: u32 }
//! }
//! let model = model::Model::new(10);
//! let secret = model.secret.get();
//! ```

mod binding;
mod items;
mod observation;
mod order;
mod registrar;
mod state;
mod transaction;
mod unobserved;

pub use binding::{Binding, Unavailable};
pub use items::Items;
pub use observation::{Observation, untracked};
pub use order::{DuplicateKey, KeyedOrder};
pub use state::State;
pub use transaction::{Mutation, Transaction, with_animation, with_transaction};
pub use unobserved::Unobserved;

#[cfg(test)]
mod tests;
