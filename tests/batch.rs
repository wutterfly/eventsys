use eventsys::{EventBackend, SlotType};

#[test]
fn test_batch() {
    let mut system = EventBackend::default();

    // Register events
    system
        .register_store::<Box<(i32, i32, i32)>>(SlotType::All)
        .unwrap();
    system
        .register_store::<(i64, u64)>(SlotType::First)
        .unwrap();
    system.register_store::<u128>(SlotType::Last).unwrap();

    system
        .register_store::<u32>(SlotType::Cmp(|current, next| *next > 2 * current))
        .unwrap();

    system
        .register_store::<u64>(SlotType::AllFilter(|next| *next >= 50))
        .unwrap();

    // call all events
    system
        .new_event::<Box<(i32, i32, i32)>>(Box::new((1, 2, 3)))
        .unwrap();
    system
        .new_event::<Box<(i32, i32, i32)>>(Box::new((5, 6, 7)))
        .unwrap();

    // call first events
    system.new_event::<(i64, u64)>((-1, 1)).unwrap();
    system.new_event::<(i64, u64)>((-2, 1)).unwrap();

    // call last events
    system.new_event::<u128>(123).unwrap();
    system.new_event::<u128>(456).unwrap();

    // call compare events
    system.new_event::<u32>(2).unwrap();
    system.new_event::<u32>(5).unwrap();
    system.new_event::<u32>(9).unwrap();

    // call filter all events
    system.new_event::<u64>(51).unwrap();
    system.new_event::<u64>(29).unwrap();
    system.new_event::<u64>(999).unwrap();

    // collect triggered events
    let all_events = system
        .query_blocking::<Box<(i32, i32, i32)>>()
        .unwrap()
        .map(|x| *x)
        .collect::<Vec<_>>();

    let first_events = system
        .query_blocking::<(i64, u64)>()
        .unwrap()
        .collect::<Vec<_>>();

    let last_events = system.query_blocking::<u128>().unwrap().collect::<Vec<_>>();

    let cmp_events = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();

    let filter_events = system.query_blocking::<u64>().unwrap().collect::<Vec<_>>();

    // check all events
    assert_eq!(&all_events, &[(1, 2, 3), (5, 6, 7)]);

    // check first event
    assert_eq!(&first_events, &[(-1, 1)]);

    // check last event
    assert_eq!(&last_events, &[456]);

    // check cmp event
    assert_eq!(&cmp_events, &[5]);

    // check filter event
    assert_eq!(&filter_events, &[51, 999]);
}

#[test]
fn test_batch_max() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::Max(3)).unwrap();

    for i in 0..10u32 {
        system.new_event::<u32>(i).unwrap();
    }

    // only the newest 3 events are kept, oldest first
    let events = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&events, &[7, 8, 9]);
}

#[test]
fn test_batch_max_not_reached() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::Max(100)).unwrap();

    for i in 0..5u32 {
        system.new_event::<u32>(i).unwrap();
    }

    let events = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&events, &[0, 1, 2, 3, 4]);
}

#[test]
fn test_batch_max_one() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::Max(1)).unwrap();

    for i in 0..10u32 {
        system.new_event::<u32>(i).unwrap();
    }

    let events = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&events, &[9]);
}

#[test]
fn test_batch_max_zero_stores_nothing() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::Max(0)).unwrap();

    for i in 0..10u32 {
        system.new_event::<u32>(i).unwrap();
    }

    let query = system.query_blocking::<u32>().unwrap();
    assert_eq!(query.len(), 0);
    assert_eq!(query.count(), 0);
}

#[test]
fn test_batch_query_drains() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All).unwrap();

    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    let first = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&first, &[1, 2]);

    // events were consumed, second query is empty
    let second = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert!(second.is_empty());

    // new events can be stored after draining
    system.new_event::<u32>(3).unwrap();
    let third = system.query_blocking::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&third, &[3]);
}

#[test]
fn test_batch_query_unblocking() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All).unwrap();

    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    let query = system.query::<u32>().unwrap();
    assert_eq!(query.len(), 2);

    // events triggered while holding the query are not blocked and end up in the next batch
    system.new_event::<u32>(3).unwrap();
    system.new_event::<u32>(4).unwrap();

    assert_eq!(query.collect::<Vec<_>>(), [1, 2]);

    let next = system.query::<u32>().unwrap().collect::<Vec<_>>();
    assert_eq!(&next, &[3, 4]);

    assert_eq!(system.query::<u32>().unwrap().len(), 0);
}

#[test]
fn test_batch_query_unblocking_slot_types() {
    let mut system = EventBackend::default();

    system.register_store::<u8>(SlotType::First).unwrap();
    system.register_store::<u16>(SlotType::Last).unwrap();
    system.register_store::<u32>(SlotType::Max(2)).unwrap();
    system
        .register_store::<u64>(SlotType::AllFilter(|new| new % 2 == 0))
        .unwrap();

    for i in 1..=5 {
        system.new_event::<u8>(i).unwrap();
        system.new_event::<u16>(i.into()).unwrap();
        system.new_event::<u32>(i.into()).unwrap();
        system.new_event::<u64>(i.into()).unwrap();
    }

    assert_eq!(system.query::<u8>().unwrap().collect::<Vec<_>>(), [1]);
    assert_eq!(system.query::<u16>().unwrap().collect::<Vec<_>>(), [5]);
    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [4, 5]);
    assert_eq!(system.query::<u64>().unwrap().collect::<Vec<_>>(), [2, 4]);
}

#[test]
fn test_batch_first_and_last_after_drain() {
    let mut system = EventBackend::default();
    system.register_store::<u8>(SlotType::First).unwrap();
    system.register_store::<u16>(SlotType::Last).unwrap();

    system.new_event::<u8>(1).unwrap();
    system.new_event::<u8>(2).unwrap();
    system.new_event::<u16>(1).unwrap();
    system.new_event::<u16>(2).unwrap();

    assert_eq!(system.query::<u8>().unwrap().collect::<Vec<_>>(), [1]);
    assert_eq!(system.query::<u16>().unwrap().collect::<Vec<_>>(), [2]);

    // after draining, the slots start over
    system.new_event::<u8>(3).unwrap();
    system.new_event::<u8>(4).unwrap();
    system.new_event::<u16>(3).unwrap();
    system.new_event::<u16>(4).unwrap();

    assert_eq!(system.query::<u8>().unwrap().collect::<Vec<_>>(), [3]);
    assert_eq!(system.query::<u16>().unwrap().collect::<Vec<_>>(), [4]);
}

#[test]
fn test_batch_types_are_independent() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All).unwrap();
    system.register_store::<i32>(SlotType::All).unwrap();

    system.new_event::<u32>(1).unwrap();
    system.new_event::<i32>(-1).unwrap();
    system.new_event::<u32>(2).unwrap();

    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(system.query::<i32>().unwrap().collect::<Vec<_>>(), [-1]);
}

#[test]
fn test_batch_register_store_replaces_slot() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::All).unwrap();
    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    // registering again swaps the slot, previously stored events are discarded
    system.register_store::<u32>(SlotType::Last).unwrap();
    system.new_event::<u32>(3).unwrap();
    system.new_event::<u32>(4).unwrap();

    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [4]);
}

#[test]
fn test_batch_large_types_are_boxed_automatically() {
    // 16 bytes per event, but the type itself is a lot bigger
    let mut system = EventBackend::<16>::new();
    system.register_store::<[u64; 32]>(SlotType::All).unwrap();

    system.new_event::<[u64; 32]>([1; 32]).unwrap();
    system.new_event::<[u64; 32]>([2; 32]).unwrap();

    let events = system.query::<[u64; 32]>().unwrap().collect::<Vec<_>>();
    assert_eq!(&events, &[[1; 32], [2; 32]]);
}

#[test]
fn test_batch_heap_types() {
    let mut system = EventBackend::default();
    system.register_store::<String>(SlotType::All).unwrap();
    system.register_store::<Vec<u32>>(SlotType::All).unwrap();

    system.new_event::<String>("hello".to_owned()).unwrap();
    system.new_event::<String>("world".to_owned()).unwrap();
    system.new_event::<Vec<u32>>(vec![1, 2, 3]).unwrap();

    assert_eq!(
        system.query::<String>().unwrap().collect::<Vec<_>>(),
        ["hello", "world"]
    );
    assert_eq!(
        system.query::<Vec<u32>>().unwrap().collect::<Vec<_>>(),
        [vec![1, 2, 3]]
    );
}

#[test]
fn test_batch_query_size_hint() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All).unwrap();

    for i in 0..5u32 {
        system.new_event::<u32>(i).unwrap();
    }

    let mut query = system.query_blocking::<u32>().unwrap();
    assert_eq!(query.size_hint(), (5, Some(5)));
    assert_eq!(query.len(), 5);

    query.next();
    assert_eq!(query.size_hint(), (4, Some(4)));
    assert_eq!(query.len(), 4);
    drop(query);

    for i in 0..5u32 {
        system.new_event::<u32>(i).unwrap();
    }

    let mut query = system.query::<u32>().unwrap();
    assert_eq!(query.size_hint(), (5, Some(5)));
    assert_eq!(query.len(), 5);

    query.next();
    assert_eq!(query.size_hint(), (4, Some(4)));
    assert_eq!(query.len(), 4);
}

#[test]
fn test_batch_first_accepts_new_event_after_every_kind_of_drain() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::First).unwrap();

    // drained by query_blocking
    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();
    assert_eq!(
        system.query_blocking::<u32>().unwrap().collect::<Vec<_>>(),
        [1]
    );

    // drained by query
    system.new_event::<u32>(3).unwrap();
    system.new_event::<u32>(4).unwrap();
    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [3]);

    // drained by cleanup
    system.new_event::<u32>(5).unwrap();
    system.new_event::<u32>(6).unwrap();
    system.cleanup();
    system.new_event::<u32>(7).unwrap();
    system.new_event::<u32>(8).unwrap();
    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [7]);

    // query without any consumed event still resets the slot
    system.new_event::<u32>(9).unwrap();
    drop(system.query_blocking::<u32>().unwrap());
    system.new_event::<u32>(10).unwrap();
    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [10]);
}

#[test]
fn test_batch_first_threads_keep_exactly_one() {
    const THREADS: u32 = 8;
    const PER_THREAD: u32 = 1_000;

    let mut system = EventBackend::<16>::new();
    system.register_store::<u32>(SlotType::First).unwrap();

    for round in 0..3 {
        std::thread::scope(|s| {
            for t in 0..THREADS {
                let system = &system;
                s.spawn(move || {
                    for i in 0..PER_THREAD {
                        system.new_event::<u32>(t * PER_THREAD + i).unwrap();
                    }
                });
            }
        });

        // exactly one event is kept per round, no matter which thread was first
        let events = system.query::<u32>().unwrap().collect::<Vec<_>>();
        assert_eq!(events.len(), 1, "round {round}");
    }
}
