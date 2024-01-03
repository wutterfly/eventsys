use eventsys::{EventBackend, SlotType};

#[test]
fn test_disable_enable() {
    let mut system = EventBackend::default();

    // Register events
    system
        .register_store::<(i32, i32, i32)>(SlotType::All)
        .unwrap();
    system.register_store::<(i64, u64)>(SlotType::All).unwrap();
    system.register_store::<u128>(SlotType::All).unwrap();

    // disable double events
    system.disable::<(i64, u64)>().unwrap();

    // call triple events
    system.new_event::<(i32, i32, i32)>((1, 2, 3)).unwrap();
    system.new_event::<(i32, i32, i32)>((5, 6, 7)).unwrap();

    // call double events
    system.new_event::<(i64, u64)>((-1, 1)).unwrap();

    // enable double events
    system.enable::<(i64, u64)>().unwrap();
    system.new_event::<(i64, u64)>((-2, 1)).unwrap();

    // call single events
    system.new_event::<u128>(123).unwrap();
    system.new_event::<u128>(456).unwrap();

    // collect triggered events
    let key_events = system
        .query_blocking::<(i32, i32, i32)>()
        .unwrap()
        .collect::<Vec<_>>();

    let pair_events = system
        .query_blocking::<(i64, u64)>()
        .unwrap()
        .collect::<Vec<_>>();

    let single_events = system.query_blocking::<u128>().unwrap().collect::<Vec<_>>();

    // check triple events
    assert_eq!(&key_events, &[(1, 2, 3), (5, 6, 7)]);

    // check pair events
    assert_eq!(&pair_events, &[(-2, 1)]);

    // check single events
    assert_eq!(&single_events, &[123, 456]);
}
