//! Tests that events are dropped exactly once, at the expected time.
//!
//! An `Arc<()>` is used as event, its strong count shows how many events are alive.

use std::sync::Arc;

use eventsys::{EventBackend, SlotType};

#[test]
fn test_stored_events_are_alive_until_queried() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All);

    let probe = Arc::new(());

    for _ in 0..3 {
        system.new_event(probe.clone()).unwrap();
    }
    assert_eq!(Arc::strong_count(&probe), 4);

    let mut consumed = system.consume::<Arc<()>>().unwrap();
    let taken = consumed.next().unwrap();
    assert_eq!(Arc::strong_count(&probe), 4);

    // unconsumed events get dropped together with the consumed events
    drop(consumed);
    assert_eq!(Arc::strong_count(&probe), 2);

    drop(taken);
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_consume_drops_unconsumed_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All);

    let probe = Arc::new(());

    for _ in 0..3 {
        system.new_event(probe.clone()).unwrap();
    }

    let mut consumed = system.consume::<Arc<()>>().unwrap();
    // the events moved into the consumed events
    assert_eq!(consumed.len(), 3);
    drop(consumed.next());
    assert_eq!(Arc::strong_count(&probe), 3);

    drop(consumed);
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_replaced_events_are_dropped() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::Last);

    let probe = Arc::new(());

    for _ in 0..10 {
        system.new_event(probe.clone()).unwrap();
    }

    // only the last one is kept alive
    assert_eq!(Arc::strong_count(&probe), 2);
}

#[test]
fn test_first_slot_drops_rejected_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::First);

    let probe = Arc::new(());

    for _ in 0..10 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 2);
}

#[test]
fn test_filter_slot_drops_rejected_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::AllFilter(|_| false));

    let probe = Arc::new(());

    for _ in 0..10 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_max_slot_drops_evicted_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::Max(4));

    let probe = Arc::new(());

    for _ in 0..100 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 5);
}

#[test]
fn test_disabled_events_are_dropped() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All);
    system.disable_all();

    let probe = Arc::new(());

    for _ in 0..3 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_unregistered_event_is_returned_not_leaked() {
    let system = EventBackend::default();

    let probe = Arc::new(());

    let err = system.new_event(probe.clone()).unwrap_err();
    assert_eq!(Arc::strong_count(&probe), 2);

    drop(err.into_inner());
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_clear_drops_stored_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All);

    let probe = Arc::new(());

    for _ in 0..5 {
        system.new_event(probe.clone()).unwrap();
    }
    assert_eq!(Arc::strong_count(&probe), 6);

    system.clear();
    assert_eq!(Arc::strong_count(&probe), 1);

    // the slot is still usable after clear
    system.new_event(probe.clone()).unwrap();
    assert_eq!(system.consume::<Arc<()>>().unwrap().count(), 1);
}

#[test]
fn test_clear_keeps_listeners() {
    let mut system = EventBackend::default();

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let calls = calls.clone();
        system.register_listener::<u32>(move |_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
    }

    system.clear();

    // the listener is still registered, and still called
    system.new_event(1u32).unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(Arc::strong_count(&calls), 2);
}

#[test]
fn test_clear_keeps_registered_types() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::Last);
    system.register_listener::<u64>(|_| {});

    system.clear();

    assert!(system.new_event(1u32).is_ok());
    assert!(system.new_event(2u64).is_ok());
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [1]);
}

#[test]
fn test_backend_drop_drops_stored_events() {
    let probe = Arc::new(());

    {
        let mut system = EventBackend::default();
        system.register_store::<Arc<()>>(SlotType::All);

        for _ in 0..5 {
            system.new_event(probe.clone()).unwrap();
        }
        assert_eq!(Arc::strong_count(&probe), 6);
    }

    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_clear_on_empty_backend() {
    let mut system = EventBackend::default();
    system.clear();

    system.register_listener::<u32>(|_| {});
    system.clear();
}
