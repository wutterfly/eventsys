use std::sync::OnceLock;

use eventsys::{EventBackend, SlotType};

static EVENTS: OnceLock<EventBackend> = OnceLock::new();

pub fn set_events(events: EventBackend) {
    EVENTS.set(events).unwrap();
}

pub fn get_events() -> &'static EventBackend {
    EVENTS.get().unwrap()
}

/// Example Event
#[derive(Debug, Clone, Copy)]
struct MouseEvent {
    _x: i32,
    _y: i32,
}

/// Example Event
#[derive(Debug, Clone, Copy)]
struct KeyboardEvent {
    _key: char,
    _up: bool,
}

fn main() {
    // create event system
    // events can be a max size of 24 bytes
    let mut system = EventBackend::new();

    // create listener to be called on event trigger
    let listener = |event: &KeyboardEvent| {
        println!("Event listener: {event:?}");
    };

    // register listener for event type
    system.register_listener::<KeyboardEvent>(listener).unwrap();

    // register event type
    system.register_store::<MouseEvent>(SlotType::All).unwrap();

    // set global event system
    println!("{:?}", system);
    EVENTS.set(system).unwrap();

    // trigger keyboard events from other thread
    _ = std::thread::spawn(|| {
        get_events()
            .new_event(KeyboardEvent {
                _key: 'A',
                _up: false,
            })
            .unwrap();
        get_events()
            .new_event(KeyboardEvent {
                _key: 'A',
                _up: true,
            })
            .unwrap();
    })
    .join();

    // trigger mouse events
    get_events()
        .new_event(MouseEvent { _x: 0, _y: 20 })
        .unwrap();
    get_events()
        .new_event(MouseEvent { _x: 20, _y: 20 })
        .unwrap();

    // query mouse event
    for mouse_events in get_events().query_blocking::<MouseEvent>().unwrap() {
        println!("Batched event: {mouse_events:?}");
    }
}
