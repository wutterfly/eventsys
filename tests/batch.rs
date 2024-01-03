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
