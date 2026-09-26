//! Tests that events are dropped exactly once, at the expected time.
//!
//! An `Arc<()>` is used as event, its strong count shows how many events are alive.

use std::sync::Arc;

use eventsys::{EventBackend, SlotType};

#[test]
fn test_stored_events_are_alive_until_queried() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All).unwrap();

    let probe = Arc::new(());

    for _ in 0..3 {
        system.new_event(probe.clone()).unwrap();
    }
    assert_eq!(Arc::strong_count(&probe), 4);

    let mut query = system.query_blocking::<Arc<()>>().unwrap();
    let taken = query.next().unwrap();
    assert_eq!(Arc::strong_count(&probe), 4);

    // unconsumed events get dropped together with the query
    drop(query);
    assert_eq!(Arc::strong_count(&probe), 2);

    drop(taken);
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_unblocking_query_drops_unconsumed_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All).unwrap();

    let probe = Arc::new(());

    for _ in 0..3 {
        system.new_event(probe.clone()).unwrap();
    }

    let mut query = system.query::<Arc<()>>().unwrap();
    // the events moved into the query
    assert_eq!(query.len(), 3);
    drop(query.next());
    assert_eq!(Arc::strong_count(&probe), 3);

    drop(query);
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_replaced_events_are_dropped() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::Last).unwrap();

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
    system.register_store::<Arc<()>>(SlotType::First).unwrap();

    let probe = Arc::new(());

    for _ in 0..10 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 2);
}

#[test]
fn test_filter_slot_drops_rejected_events() {
    let mut system = EventBackend::default();
    system
        .register_store::<Arc<()>>(SlotType::AllFilter(|_| false))
        .unwrap();

    let probe = Arc::new(());

    for _ in 0..10 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_max_slot_drops_evicted_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::Max(4)).unwrap();

    let probe = Arc::new(());

    for _ in 0..100 {
        system.new_event(probe.clone()).unwrap();
    }

    assert_eq!(Arc::strong_count(&probe), 5);
}

#[test]
fn test_disabled_events_are_dropped() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All).unwrap();
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
fn test_cleanup_drops_stored_events() {
    let mut system = EventBackend::default();
    system.register_store::<Arc<()>>(SlotType::All).unwrap();

    let probe = Arc::new(());

    for _ in 0..5 {
        system.new_event(probe.clone()).unwrap();
    }
    assert_eq!(Arc::strong_count(&probe), 6);

    system.cleanup();
    assert_eq!(Arc::strong_count(&probe), 1);

    // the slot is still usable after cleanup
    system.new_event(probe.clone()).unwrap();
    assert_eq!(system.query::<Arc<()>>().unwrap().count(), 1);
}

#[test]
fn test_cleanup_drops_listeners() {
    let mut system = EventBackend::default();

    let probe = Arc::new(());
    {
        let probe = probe.clone();
        system
            .register_listener::<u32>(move |_| _ = &probe)
            .unwrap();
    }
    assert_eq!(Arc::strong_count(&probe), 2);

    system.cleanup();
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_backend_drop_drops_stored_events() {
    let probe = Arc::new(());

    {
        let mut system = EventBackend::default();
        system.register_store::<Arc<()>>(SlotType::All).unwrap();

        for _ in 0..5 {
            system.new_event(probe.clone()).unwrap();
        }
        assert_eq!(Arc::strong_count(&probe), 6);
    }

    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_cleanup_on_empty_backend() {
    let mut system = EventBackend::default();
    system.cleanup();

    system.register_listener::<u32>(|_| {}).unwrap();
    system.cleanup();
}
