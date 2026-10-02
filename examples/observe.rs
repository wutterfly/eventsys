//! Observing events: looking at them without taking them out, so everyone sees the same ones.

use eventsys::{EventBackend, SlotType};

struct Key(char);

fn main() {
    // set up
    let mut events = EventBackend::new();
    events.register_store::<Key>(SlotType::All);

    // trigger events
    events.new_event(Key('h')).unwrap();
    events.new_event(Key('i')).unwrap();

    // observe them: as often as you like, they are still there
    for key in events.observe::<Key>().unwrap() {
        println!("first:  {}", key.0);
    }
    for key in events.observe::<Key>().unwrap() {
        println!("second: {}", key.0);
    }

    // start a new round: the observed events are dropped
    events.reset();
}
