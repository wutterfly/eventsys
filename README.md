# EventSys

[![Rust](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml/badge.svg)](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml)

**Pre-1.0:** the API can still change between `0.x` releases. The [changelog](./CHANGELOG.md) lists what changed.

A library for dispatching and processing events. Events can be handled in a deferred and/or immediate way.

Events can be:

- handled with an event listener: `EventBackend::register_listener()`
- stored, to be handled later as a batch: `EventBackend::register_store()`

To trigger a new event, call `EventBackend::new_event()`. Stored events are taken out with `EventBackend::consume()`,
or looked at, by any number of readers, with `EventBackend::observe()`.

## Installation

```sh
cargo add eventsys
```

It needs Rust 1.88 or newer.

## Using an `EventBackend`

Using an `EventBackend` should generally be done in 2 phases:

- Create a new `EventBackend` and register all events
- Use the `EventBackend` to trigger new events

Events can be triggered without needing mutable access to the `EventBackend`,
so it can be shared between threads (for example in a `static`).
Registering new event types does need mutable access.

Every event type has to be registered before it can be triggered. Any type that is `Send + RefUnwindSafe + 'static`
can be used as an event, regardless of its size.

## Example

### Example Listeners

```rust
use eventsys::EventBackend;
// create event system
let mut system = EventBackend::new();

// create listener to be called on event trigger
let listener = |event: &u32| {
    // handle event
};

// register listener for event type
system.register_listener::<u32>(listener);

// trigger event
system.new_event::<u32>(123).unwrap();
```

### Example Consuming

```rust
use eventsys::{EventBackend, SlotType};
// create event system
let mut system = EventBackend::new();

// register event type to be stored
system.register_store::<u32>(SlotType::All);

// trigger event
system.new_event::<u32>(123).unwrap();
system.new_event::<u32>(456).unwrap();
system.new_event::<u32>(789).unwrap();

// consume batch
let event_iter = system.consume::<u32>().unwrap();

for event in event_iter {
    // handle event
}
```

Consuming takes the stored events out of the event system, so each event is part of exactly one batch. Events that are
triggered after consuming started belong to the next batch. Events that are not consumed by the time the iterator is
dropped are discarded.

## Slot Types

`SlotType` decides which events of a registered type are stored:

| `SlotType`     | Stores                                                                               |
| -------------- | ------------------------------------------------------------------------------------ |
| `All`          | Every event.                                                                         |
| `AllFilter(f)` | Every event, for which `f` returns `true`.                                           |
| `Max(n)`       | Up to `n` events; any more replace the oldest events.                                |
| `Last`         | Only the most recent event.                                                          |
| `First`        | Only the first event, until it is consumed; any other events are discarded.          |
| `Cmp(f)`       | One event; a new event replaces the stored one, if `f(current, new)` returns `true`. |

`All` and `AllFilter` accept events from multiple threads without them waiting for each other. The order of events
is only guaranteed per thread, not between threads. `Last` is lock-free.

## Observing Events in Several Places

Consuming events takes them out of the event system, so only one place can handle them. If several parts of a program
need to see the same events (for example the systems of a game engine), observe them instead:

```rust
use eventsys::{EventBackend, SlotType};

let mut system = EventBackend::new();
system.register_store::<u32>(SlotType::All);

// trigger events
system.new_event::<u32>(1).unwrap();
system.new_event::<u32>(2).unwrap();

// the first observe takes the events out of the event system, any number of observers see the same ones
assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);

// start a new round, the next observe fetches the events that were triggered since
system.reset();
assert!(system.observe::<u32>().unwrap().as_slice().is_empty());
```

`consume` and `observe` both hand out the events of a type, but differ in who gets them:

|                  | `EventBackend::consume`            | `EventBackend::observe`                     |
| ---------------- | ---------------------------------- | ------------------------------------------- |
| Returns          | `Consumed`, an iterator of `T`     | `Observed`, an iterator of `&T`             |
| Events           | taken out of the event system      | stay in the event system, shared            |
| Callers          | one, the first one gets everything | any number, on any thread, all see the same |
| Calling it again | returns what was triggered since   | returns the same events until `reset`       |

Iterating is the main way to get at the events. Both types are used the same way: they know how many events are left
(`len`, `is_empty`), and, for code that needs a slice, have an `as_slice()` with the events that were not iterated over
yet. Neither is `Clone` or `Copy`: to look at observed events a second time, call `observe` again.

`observe` needs only shared access, and no event type has to be known up front: whatever is asked for, from wherever,
gets fetched on demand (so it also works for event types that are registered by plugins). The first `observe` of a type
takes its events out of the event system; every other `observe` returns the same events until `reset`. Events that are
triggered after the first `observe` of their type belong to the next round. `reset` needs mutable access, so call it
where nothing else uses the `EventBackend`; it does not need to know which types were observed. The buffers are
recycled, so after a few rounds nothing is allocated. Observing needs the event type to be `Sync`.

Events of a type that nobody observes or consumes stay in the event system, so use a slot type that limits them (`Max`,
`Last`) for events that may go unread.

## Clearing Events

`EventBackend::clear` drops all events that are currently stored (including the observed ones),
but keeps everything that was allocated for them, so storing events again does not allocate. Registered event types and
listeners are not touched.

## Errors

Triggering, consuming or observing an event type that was not registered returns an `EventError`, and so does
consuming or observing one that was registered without a store. `EventError::raw_err()` tells which it was, as a
`RawErr` that can be matched. An event that could not be triggered is given back with `EventError::into_inner()`.

```rust
use eventsys::{EventBackend, RawErr};

let events = EventBackend::new();

// nothing was registered, so the event is not lost, and the error says why
let error = events.new_event(42u32).unwrap_err();
assert!(matches!(error.raw_err(), RawErr::UnregisteredEventType { .. }));
assert_eq!(error.into_inner(), 42);
```

## Memory Usage

`EventBackend` implements [`anythingy::HeapSize`](https://docs.rs/anythingy/latest/anythingy/trait.HeapSize.html),
to report the heap memory its registrations and stored events have allocated. It is meant for occasional checks,
not for hot paths: it visits every registered event type, and the result is a snapshot.

```rust
use anythingy::HeapSize;
use eventsys::{EventBackend, SlotType};

let mut system = EventBackend::new();
system.register_store::<u32>(SlotType::All);

let bytes = system.heap_size();
```

## Examples

Each example shows one thing, and is set up the same way: register the event types, trigger events, handle them.

| Example                                | Shows                                                                     | Run                             |
| -------------------------------------- | ------------------------------------------------------------------------- | ------------------------------- |
| [`listeners`](./examples/listeners.rs) | handling events right away, with a listener                               | `cargo run --example listeners` |
| [`consume`](./examples/consume.rs)     | taking events out of the event system, to handle them in one place        | `cargo run --example consume`   |
| [`observe`](./examples/observe.rs)     | looking at events without taking them out, so everyone sees the same ones | `cargo run --example observe`   |

## Licence

This project is licensed under the [MIT license](./LICENSE).
