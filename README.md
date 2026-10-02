# EventSys

[![Rust](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml/badge.svg)](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml)

**This is a work-in-progress project and not for production use.**

A library for dispatching and processing events. Events can be handled in a deferred and/or immediate way.

Events can be:
* handled with an event listener: `EventBackend::register_listener()`
* stored, to be handled later as a batch: `EventBackend::register_store()`

To trigger a new event, call `EventBackend::new_event()`.

## Using an `EventBackend`

Using an `EventBackend` should generally be done in 2 phases:
* Create a new `EventBackend` and register all events
* Use the `EventBackend` to trigger new events

Events can be triggered without needing mutable access to the `EventBackend`,
so it can be shared between threads (for example in a `static`, see [`examples/static.rs`](./examples/static.rs)).
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

### Example Batching
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

// query batch
let event_iter = system.query::<u32>().unwrap();

for event in event_iter {
    // handle event
}
```

A query takes the stored events out of the event system, so each event is part of exactly one batch. Events that are
triggered after a query started belong to the next batch. Events that are not consumed by the time the query is
dropped are discarded.

## Slot Types

`SlotType` decides which events of a registered type are stored:

| `SlotType`        | Stores                                                                                  |
|-------------------|-----------------------------------------------------------------------------------------|
| `All`             | Every event.                                                                            |
| `AllFilter(f)`    | Every event, for which `f` returns `true`.                                              |
| `Max(n)`          | Up to `n` events; any more replace the oldest events.                                  |
| `Last`            | Only the most recent event.                                                             |
| `First`           | Only the first event, until it is consumed; any other events are discarded.             |
| `Cmp(f)`          | One event; a new event replaces the stored one, if `f(current, new)` returns `true`.    |

`All` and `AllFilter` accept events from multiple threads without them waiting for each other. The order of events
is only guaranteed per thread, not between threads. `Last` is lock-free.

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

## Licence
This project is licensed under the [MIT license](./LICENCE).
