# EventSys

[![Rust](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml/badge.svg)](https://github.com/wutterfly/eventsys/actions/workflows/rust.yml)

**This is a work-in-progess project and not for production use.**

A library for dispatching and processing events. Events can be handled in a deferred and/or immediate way.

Events can be:
* handled with an event listener: `EventBackend::register_listener()`
* be registered: `EventBackend::register_store()`

To trigger a new event, call `EventBackend::new_event()`.

## Using an `EventBackend`

Using an `EventBackend` should generally be done in 2 phases:
* Create an new `EventBackend` and register all events
* Use the `EventBackend` to trigger new events

Events can be triggered without needing mutable access to the `EventBackend`,
while registering new event types does need mutable access.

## Example

### Example Listeners
```rust
use eventsys::EventBackend;
// create event system, events can be a max size of 16 bytes
let mut system = EventBackend::<16>::new();

// create listener to be called on event trigger
let listener = |event: &u32| {
    // handle event
};

// register listener for event type
system.register_listener::<u32>(listener).unwrap();

// trigger event
system.new_event::<u32>(123);
```

### Example Batching
```rust
use eventsys::{EventBackend, SlotType};
// create event system, events can be a max size of 16 bytes
let mut system = EventBackend::<16>::new();
    
// register listener for event type
system.register_store::<u32>(SlotType::All).unwrap();

// trigger event
system.new_event::<u32>(123);
system.new_event::<u32>(456);
system.new_event::<u32>(789);

// query batch
let event_iter = system.query::<u32>().unwrap();

for event in event_iter {
    // handle event
}
```


## Licence
This project is licensed under the [MIT license](./LICENSE).