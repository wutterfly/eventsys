//! # Eventsys
//!
//! A library for dispatching events and processing events. Events can be handled in a deferred and/or immediate way.
//!
//! Events can be:
//!    * handled with an event listener: [`EventBackend::register_listener()`]
//!    * be registered: [`EventBackend::register_store()`]
//!
//! To trigger a new event, call [`EventBackend::new_event()`].
//!
//! ## Using an [`EventBackend`]
//!
//! Using an [`EventBackend`] should generally be done in 2 phases:
//!    * Create a new [`EventBackend`] and register all events
//!    * Use the [`EventBackend`] to trigger new events
//!
//! Events can be triggered without needing mutable access to the [`EventBackend`],
//! while registering new event types does need mutable access.
//!
//! ## Listeners
//! The most direct method to handle events is registering a function to an event type and let this function be called when the corresponding
//! event is triggered.
//!
//! ### Example Listener
//!
//! ```rust
//! use eventsys::EventBackend;
//!
//! // create event system
//! let mut system = EventBackend::new();
//!
//! // create listener to be called on event trigger
//! let listener = |event: &u32| {
//!     // handle event
//! };
//!
//! // register listener for event type
//! system.register_listener::<u32>(listener);
//!
//! // trigger event
//! system.new_event::<u32>(123);
//! ```
//! ## Batching
//!
//! Sometimes it is not desired to process events right away. The second way to handle events is to store them
//! and process them as batch.
//!
//! ### Example Batching
//!
//! ```rust
//! use eventsys::{EventBackend, SlotType};
//!
//! // create event system
//! let mut system = EventBackend::new();
//!
//! // register listener for event type
//! system.register_store::<u32>(SlotType::All);
//!
//! // trigger event
//! system.new_event::<u32>(123);
//! system.new_event::<u32>(456);
//! system.new_event::<u32>(789);
//!
//! // consume batch
//! let event_iter = system.consume::<u32>().unwrap();
//!
//! for event in event_iter {
//!     // handle event
//! }
//! ```
//!
//! ## Observing Events in Several Places
//!
//! Consuming events with [`EventBackend::consume()`] takes them out of the event system, so only one place can
//! handle them. If several parts of a program need to see the same events, observe them instead.
//!
//! The first [`EventBackend::observe()`] of an event type takes its events out of the event system and keeps them in
//! a buffer. Every other call returns the same events, from any thread, until [`EventBackend::reset()`] starts a new
//! round. Observing needs only shared access, and no event type has to be known or requested up front: whatever is
//! asked for gets fetched on demand. Only resetting needs mutable access.
//!
//! ### Example Observing
//!
//! ```rust
//! use eventsys::{EventBackend, SlotType};
//!
//! let mut system = EventBackend::new();
//! system.register_store::<u32>(SlotType::All);
//!
//! // trigger events
//! system.new_event::<u32>(1).unwrap();
//! system.new_event::<u32>(2).unwrap();
//!
//! // the first observe takes the events out of the event system, any number of observers see the same ones
//! assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
//! assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
//!
//! // start a new round, the next observe fetches the events that were triggered since
//! system.reset();
//! assert!(system.observe::<u32>().unwrap().as_slice().is_empty());
//! ```

#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::module_name_repetitions)]

mod backend;
mod consumed;
mod err;
mod fetched;
mod map;
mod observed;
mod slot;

pub use backend::EventBackend;
pub use consumed::Consumed;
pub use err::{EventError, NoValue, RawErr, Value};
pub use observed::Observed;
pub use slot::SlotType;
