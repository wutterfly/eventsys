//! Handling events right away, with a listener.

use eventsys::EventBackend;

struct Key(char);

fn main() {
    // set up
    let mut events = EventBackend::new();
    events.register_listener::<Key>(|key| println!("pressed {}", key.0));

    // trigger events: the listener is called right away
    events.new_event(Key('h')).unwrap();
    events.new_event(Key('i')).unwrap();
}
