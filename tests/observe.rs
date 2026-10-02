//! Tests for observing events: events that are fetched lazily, on first use, and can be observed by any number of observers.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anythingy::HeapSize;
use eventsys::{EventBackend, SlotType};

#[derive(Debug, PartialEq, Eq)]
struct Damage(u32);

#[test]
fn test_first_get_fetches_the_events() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    system.new_event(2u32).unwrap();

    // nothing was requested up front, and no mutable access is needed
    let system = &system;
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
}

#[test]
fn test_observe_returns_the_same_events_as_often_as_wanted() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    system.new_event(2u32).unwrap();

    for _ in 0..3 {
        assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
    }
}

#[test]
fn test_observe_takes_the_events_out_of_the_event_system() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    system.observe::<u32>().unwrap();

    assert_eq!(system.consume::<u32>().unwrap().len(), 0);
}

#[test]
fn test_events_triggered_after_the_first_get_wait_for_the_next_round() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);

    system.new_event(2u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);

    system.reset();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [2]);
}

#[test]
fn test_reset_starts_a_new_round() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);

    system.reset();
    system.new_event(2u32).unwrap();
    system.new_event(3u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [2, 3]);

    // nothing was triggered during the round
    system.reset();
    assert!(system.observe::<u32>().unwrap().as_slice().is_empty());
}

#[test]
fn test_reset_before_get_loses_nothing() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    system.reset();
    system.reset();

    // events wait in the event system, until somebody asks for them
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);
}

#[test]
fn test_reset_does_not_need_to_know_the_types() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<Damage>(SlotType::All);
    system.register_store::<u64>(SlotType::Last);

    system.new_event(1u32).unwrap();
    system.new_event(Damage(2)).unwrap();
    system.new_event(3u64).unwrap();

    // some types are asked for, some are not
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);
    assert_eq!(system.observe::<Damage>().unwrap().as_slice(), [Damage(2)]);

    system.reset();

    assert!(system.observe::<u32>().unwrap().as_slice().is_empty());
    assert!(system.observe::<Damage>().unwrap().as_slice().is_empty());
    assert_eq!(system.observe::<u64>().unwrap().as_slice(), [3]);
}

#[test]
fn test_types_that_are_not_asked_for_keep_their_events() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<Damage>(SlotType::All);

    system.new_event(1u32).unwrap();
    system.new_event(Damage(2)).unwrap();

    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);
    system.reset();

    // nobody asked for `Damage`, so its events are still there
    assert_eq!(
        system.consume::<Damage>().unwrap().collect::<Vec<_>>(),
        [Damage(2)]
    );
}

#[test]
fn test_event_types_are_independent() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<Damage>(SlotType::All);

    system.new_event(1u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);

    // `Damage` is fetched later, with whatever was triggered by then
    system.new_event(Damage(2)).unwrap();
    system.new_event(2u32).unwrap();
    assert_eq!(system.observe::<Damage>().unwrap().as_slice(), [Damage(2)]);
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);
}

#[test]
fn test_threads_that_ask_first_at_the_same_time_see_the_same_events() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    for i in 0..1000u32 {
        system.new_event(i).unwrap();
    }

    // every thread asks for the events for the first time, no one asked before
    let mut seen = std::thread::scope(|s| {
        let readers = (0..8)
            .map(|_| s.spawn(|| system.observe::<u32>().unwrap()))
            .collect::<Vec<_>>();

        readers
            .into_iter()
            .map(|reader| reader.join().unwrap().as_slice().to_vec())
            .collect::<Vec<_>>()
    });

    // the events were taken out of the event system once, and everyone got all of them
    let expected = (0..1000).collect::<Vec<_>>();
    for events in &mut seen {
        events.sort_unstable();
        assert_eq!(*events, expected);
    }
    assert_eq!(system.consume::<u32>().unwrap().len(), 0);
}

#[test]
fn test_events_can_be_triggered_while_others_read() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    for i in 0..100u32 {
        system.new_event(i).unwrap();
    }
    // the first observe takes the events of this round
    system.observe::<u32>().unwrap();

    let expected = (0..100).collect::<Vec<_>>();

    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..100 {
                    assert_eq!(system.observe::<u32>().unwrap().as_slice(), expected);
                }
            });
        }

        for t in 0..4u32 {
            let system = &system;
            s.spawn(move || {
                for i in 0..100 {
                    system.new_event(1000 + t * 100 + i).unwrap();
                }
            });
        }
    });

    // everything that was triggered meanwhile is part of the next round
    system.reset();
    assert_eq!(system.observe::<u32>().unwrap().as_slice().len(), 400);
}

#[test]
fn test_every_slot_type_can_be_observed() {
    let mut system = EventBackend::new();
    system.register_store::<u8>(SlotType::All);
    system.register_store::<u16>(SlotType::AllFilter(|e| e % 2 == 0));
    system.register_store::<u32>(SlotType::Last);
    system.register_store::<u64>(SlotType::First);
    system.register_store::<i8>(SlotType::Max(2));
    system.register_store::<i16>(SlotType::Cmp(|current, new| new > current));

    for i in 1..=3 {
        system.new_event(i as u8).unwrap();
        system.new_event(i as u16).unwrap();
        system.new_event(i as u32).unwrap();
        system.new_event(i as u64).unwrap();
        system.new_event(i as i8).unwrap();
        system.new_event(i as i16).unwrap();
    }

    assert_eq!(system.observe::<u8>().unwrap().as_slice(), [1, 2, 3]);
    assert_eq!(system.observe::<u16>().unwrap().as_slice(), [2]);
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [3]);
    assert_eq!(system.observe::<u64>().unwrap().as_slice(), [1]);
    assert_eq!(system.observe::<i8>().unwrap().as_slice(), [2, 3]);
    assert_eq!(system.observe::<i16>().unwrap().as_slice(), [3]);
}

#[test]
fn test_observe_unregistered_type_fails() {
    let system = EventBackend::new();
    assert!(system.observe::<u32>().is_err());
}

#[test]
fn test_observe_type_with_only_listener_fails() {
    let mut system = EventBackend::new();
    system.register_listener::<u32>(|_| {});

    let err = system.observe::<u32>().unwrap_err();
    assert!(err.to_string().contains("not registered to store"), "{err}");
}

#[test]
fn test_types_registered_later_can_be_observed() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);
    system.new_event(1u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);

    // for example a plugin, that gets loaded later
    assert!(system.observe::<Damage>().is_err());
    system.register_store::<Damage>(SlotType::All);
    system.new_event(Damage(2)).unwrap();

    assert_eq!(system.observe::<Damage>().unwrap().as_slice(), [Damage(2)]);
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1]);
}

#[test]
fn test_fetched_events_are_dropped_by_reset() {
    let mut system = EventBackend::new();
    system.register_store::<Arc<()>>(SlotType::All);

    let probe = Arc::new(());
    system.new_event(probe.clone()).unwrap();
    assert_eq!(Arc::strong_count(&probe), 2);

    // fetched events are alive, for as long as they can be observed
    system.observe::<Arc<()>>().unwrap();
    assert_eq!(Arc::strong_count(&probe), 2);

    system.reset();
    assert_eq!(Arc::strong_count(&probe), 1);
}

#[test]
fn test_buffers_are_recycled() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    for i in 0..1000u32 {
        system.new_event(i).unwrap();
    }
    system.observe::<u32>().unwrap();
    let with_events = system.heap_size();
    assert!(with_events >= size_of::<u32>() * 1000);

    // resetting drops the events, but keeps the buffer for the next round
    system.reset();
    assert_eq!(system.heap_size(), with_events);

    // the buffer gets used again: after a few rounds of the same size, memory does not grow any more
    let mut sizes = Vec::new();
    for _ in 0..5 {
        for i in 0..1000u32 {
            system.new_event(i).unwrap();
        }
        assert_eq!(system.observe::<u32>().unwrap().as_slice().len(), 1000);
        sizes.push(system.heap_size());

        system.reset();
    }

    assert_eq!(sizes[3], sizes[4]);
    assert_eq!(system.heap_size(), sizes[4]);
}

#[test]
fn test_listeners_still_run_immediately() {
    let calls = Arc::new(AtomicUsize::new(0));

    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    let calls_c = calls.clone();
    system.register_listener::<u32>(move |_| {
        calls_c.fetch_add(1, Ordering::Relaxed);
    });

    system.new_event(1u32).unwrap();
    system.new_event(2u32).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [1, 2]);
}

#[test]
fn test_heap_size_counts_the_fetched_events() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::Last);
    let before = system.heap_size();

    // `Last` stores a single event, so the events can only come from the fetched buffer
    system.new_event(1u32).unwrap();
    system.observe::<u32>().unwrap();

    assert!(system.heap_size() > before);
}

#[test]
fn test_clear_drops_the_observed_events() {
    let mut system = EventBackend::new();
    system.register_store::<Arc<()>>(SlotType::All);

    let probe = Arc::new(());
    system.new_event(probe.clone()).unwrap();
    system.observe::<Arc<()>>().unwrap();
    system.reset();
    system.new_event(probe.clone()).unwrap();
    system.observe::<Arc<()>>().unwrap();
    system.new_event(probe.clone()).unwrap();
    assert_eq!(Arc::strong_count(&probe), 3);

    system.clear();
    assert_eq!(Arc::strong_count(&probe), 1);
    assert!(system.observe::<Arc<()>>().unwrap().as_slice().is_empty());
}

#[test]
fn test_mixing_consume_and_observe_splits_the_events() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);

    system.new_event(1u32).unwrap();
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [1]);

    system.new_event(2u32).unwrap();
    assert_eq!(system.observe::<u32>().unwrap().as_slice(), [2]);
}

#[test]
fn test_events_that_are_not_sync_can_still_be_stored_in_the_backend() {
    /// `Send`, but not `Sync`: can be triggered and consumed, but not observed by several observers.
    struct NotSync(#[allow(dead_code)] *const u8);

    // SAFETY: the pointer is never dereferenced.
    unsafe impl Send for NotSync {}

    let mut system = EventBackend::new();
    system.register_store::<NotSync>(SlotType::All);
    system.new_event(NotSync(std::ptr::null())).unwrap();

    // observing needs `Sync`, but sharing the `EventBackend` itself must work for any `Send` event
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EventBackend>();

    assert_eq!(system.consume::<NotSync>().unwrap().len(), 1);
}

/// `Consumed` and `Observed` are meant to be used the same way, so everything one can do, the other can do as well.
/// The same code is run for both, so changing the API of only one of them does not compile.
macro_rules! check_the_same_api {
    ($iter:expr, $item:ty) => {{
        let mut iter = $iter;

        // iterating, and knowing how many events are left
        let _: &dyn ExactSizeIterator<Item = $item> = &iter;
        assert_eq!(iter.len(), 3);
        assert!(!iter.is_empty());
        assert_eq!(iter.size_hint(), (3, Some(3)));

        // showing the events that are left, as a slice
        assert_eq!(iter.as_slice(), [1, 2, 3]);

        // the debug output shows the events that are left
        let debug = format!("{iter:?}");
        assert!(debug.ends_with("([1, 2, 3])"), "{debug}");

        // iterating removes the first event from all of them
        assert!(iter.next().is_some());
        assert_eq!(iter.len(), 2);
        assert_eq!(iter.as_slice(), [2, 3]);

        iter.by_ref().for_each(drop);
        assert!(iter.is_empty());
        assert_eq!(iter.as_slice().len(), 0);
    }};
}

#[test]
fn test_consumed_and_observed_have_the_same_api() {
    let mut system = EventBackend::new();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<u64>(SlotType::All);

    for i in 1..=3 {
        system.new_event(i as u32).unwrap();
        system.new_event(i as u64).unwrap();
    }

    check_the_same_api!(system.consume::<u32>().unwrap(), u32);
    check_the_same_api!(system.observe::<u64>().unwrap(), &u64);
}
