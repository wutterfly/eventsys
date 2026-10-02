//! Tests with more event types than are looked up by scanning: the registrations past that are looked up with a hash
//! map, and everything has to work the same for them.

use anythingy::HeapSize;
use eventsys::{EventBackend, SlotType};

/// Distinct event type for every `N`.
struct Msg<const N: usize>(usize);

macro_rules! types {
    ($($n:literal),* $(,)?) => {
        /// The number of event types.
        const TYPES: usize = [$($n),*].len();

        fn register(events: &mut EventBackend) {
            $(
                events.register_store::<Msg<$n>>(SlotType::Last);
            )*
        }

        /// Triggers one event of the type with the given number, with its number as value.
        fn fire(events: &EventBackend, n: usize) {
            match n {
                $( $n => events.new_event(Msg::<$n>($n)).unwrap(), )*
                _ => panic!("no such type: {n}"),
            }
        }

        fn consume(events: &EventBackend, n: usize) -> Vec<usize> {
            match n {
                $( $n => events.consume::<Msg<$n>>().unwrap().map(|m| m.0).collect(), )*
                _ => panic!("no such type: {n}"),
            }
        }

        fn observe(events: &EventBackend, n: usize) -> Vec<usize> {
            match n {
                $( $n => events.observe::<Msg<$n>>().unwrap().map(|m| m.0).collect(), )*
                _ => panic!("no such type: {n}"),
            }
        }

        fn is_registered(events: &EventBackend, n: usize) -> bool {
            match n {
                $( $n => events.consume::<Msg<$n>>().is_ok(), )*
                _ => false,
            }
        }
    };
}

types!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63
);

fn backend() -> EventBackend {
    let mut events = EventBackend::new();
    register(&mut events);
    events
}

#[test]
fn test_every_type_is_found_wherever_it_is_stored() {
    let events = backend();

    for n in 0..TYPES {
        assert!(is_registered(&events, n), "type {n} is not registered");
    }
}

#[test]
fn test_events_of_every_type_arrive_at_the_right_type() {
    let events = backend();

    for n in 0..TYPES {
        fire(&events, n);
    }

    for n in 0..TYPES {
        assert_eq!(consume(&events, n), [n], "type {n}");
    }

    // everything was taken
    for n in 0..TYPES {
        assert_eq!(consume(&events, n), [] as [usize; 0], "type {n}");
    }
}

#[test]
fn test_every_type_can_be_observed_and_reset() {
    let mut events = backend();

    for n in 0..TYPES {
        fire(&events, n);
    }
    for n in 0..TYPES {
        assert_eq!(observe(&events, n), [n]);
        assert_eq!(observe(&events, n), [n]);
    }

    events.reset();

    // the next round only has what was triggered since, for every type
    for n in (0..TYPES).step_by(3) {
        fire(&events, n);
    }
    for n in 0..TYPES {
        let expected = if n % 3 == 0 { vec![n] } else { vec![] };
        assert_eq!(observe(&events, n), expected, "type {n}");
    }
}

#[test]
fn test_clear_reaches_every_type() {
    let mut events = backend();

    for n in 0..TYPES {
        fire(&events, n);
    }

    events.clear();

    for n in 0..TYPES {
        assert_eq!(consume(&events, n), [] as [usize; 0], "type {n}");
    }
}

#[test]
fn test_disable_and_enable_reach_every_type() {
    let events = backend();

    events.disable_all();
    for n in 0..TYPES {
        fire(&events, n);
    }
    for n in 0..TYPES {
        assert_eq!(
            consume(&events, n),
            [] as [usize; 0],
            "type {n} was disabled"
        );
    }

    events.enable_all();
    for n in 0..TYPES {
        fire(&events, n);
    }
    for n in 0..TYPES {
        assert_eq!(consume(&events, n), [n], "type {n} was enabled");
    }
}

#[test]
fn test_registering_a_type_again_replaces_its_store_wherever_it_is() {
    let mut events = backend();

    // the first one, and the last one
    for n in [0, TYPES - 1] {
        fire(&events, n);
    }
    events.register_store::<Msg<0>>(SlotType::First);
    events.register_store::<Msg<63>>(SlotType::First);

    assert_eq!(
        consume(&events, 0),
        [] as [usize; 0],
        "its events were dropped"
    );
    assert_eq!(consume(&events, TYPES - 1), [] as [usize; 0]);

    // and it is still the same registration, so no type was added or lost
    for n in 0..TYPES {
        assert!(is_registered(&events, n));
    }
}

#[test]
fn test_heap_size_counts_the_registrations_that_are_not_inline() {
    let few = {
        let mut events = EventBackend::new();
        events.register_store::<Msg<0>>(SlotType::Last);
        events.register_store::<Msg<1>>(SlotType::Last);
        events
    };

    // a table for the types that were not inline, on top of what every registration owns
    assert!(backend().heap_size() > few.heap_size() * 4);
}

#[test]
fn test_types_that_are_registered_later_do_not_disturb_the_earlier_ones() {
    let mut events = EventBackend::new();
    events.register_store::<Msg<0>>(SlotType::All);
    fire(&events, 0);

    // many more, which registers `Msg<0>` again, with another store
    register(&mut events);

    for n in 0..TYPES {
        fire(&events, n);
        assert_eq!(consume(&events, n), [n]);
    }
}
