//! Consuming events: taking them out of the event system, to handle them in one place.

use eventsys::{EventBackend, SlotType};

struct Key(char);

fn main() {
    // set up
    let mut events = EventBackend::new();
    events.register_store::<Key>(SlotType::All);

    // trigger events
    events.new_event(Key('h')).unwrap();
    events.new_event(Key('i')).unwrap();

    // consume them
    for key in events.consume::<Key>().unwrap() {
        println!("{}", key.0);
    }

    // they were taken out, so there is nothing left
    assert_eq!(events.consume::<Key>().unwrap().len(), 0);
}
