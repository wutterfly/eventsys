use eventsys::{EventBackend, SlotType};

/// Backend that can only hold events of up to 4 bytes.
type Small = EventBackend<4>;

#[test]
fn test_new_event_unregistered() {
    let system = EventBackend::default();

    let err = system.new_event::<u32>(42).unwrap_err();

    assert!(err.to_string().contains("Unregistered event type"));
    assert!(err.to_string().contains("u32"));

    // the rejected event can be recovered
    assert_eq!(err.into_inner(), 42);
}

#[test]
fn test_new_event_unregistered_boxed_value_is_returned() {
    let system = EventBackend::default();

    let err = system.new_event::<String>("hello".to_owned()).unwrap_err();

    assert_eq!(err.into_inner(), "hello");
}

#[test]
fn test_new_event_too_big() {
    let system = Small::new();

    let err = system.new_event::<u64>(7).unwrap_err();

    assert!(err.to_string().contains("incorrect size"));
    assert!(err.to_string().contains("max size: 4"));
    assert_eq!(err.into_inner(), 7);
}

#[test]
fn test_register_store_too_big() {
    let mut system = Small::new();

    let err = system.register_store::<u64>(SlotType::All).unwrap_err();

    assert!(err.to_string().contains("incorrect size"));
    assert!(err.to_string().contains("max size: 4"));
}

#[test]
fn test_register_listener_too_big() {
    let mut system = Small::new();

    let err = system.register_listener::<u64>(|_| {}).unwrap_err();

    assert!(err.to_string().contains("incorrect size"));
    assert!(err.to_string().contains("max size: 4"));
}

#[test]
fn test_query_too_big() {
    let system = Small::new();

    let err = system.query::<u64>().err().unwrap();
    assert!(err.to_string().contains("incorrect size"));

    let err = system.query_blocking::<u64>().err().unwrap();
    assert!(err.to_string().contains("incorrect size"));
}

#[test]
fn test_enable_disable_too_big() {
    let system = Small::new();

    let err = system.enable::<u64>().unwrap_err();
    assert!(err.to_string().contains("incorrect size"));

    let err = system.disable::<u64>().unwrap_err();
    assert!(err.to_string().contains("incorrect size"));
}

#[test]
fn test_small_types_fit() {
    let mut system = Small::new();

    system.register_store::<u8>(SlotType::All).unwrap();
    system.register_store::<u32>(SlotType::All).unwrap();

    system.new_event::<u8>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    assert_eq!(system.query::<u8>().unwrap().collect::<Vec<_>>(), [1]);
    assert_eq!(system.query::<u32>().unwrap().collect::<Vec<_>>(), [2]);
}

#[test]
fn test_query_unregistered() {
    let system = EventBackend::default();

    let err = system.query::<u32>().err().unwrap();
    assert!(err.to_string().contains("Unregistered event type"));

    let err = system.query_blocking::<u32>().err().unwrap();
    assert!(err.to_string().contains("Unregistered event type"));
}

#[test]
fn test_query_registered_without_store() {
    let mut system = EventBackend::default();
    system.register_listener::<u32>(|_| {}).unwrap();

    let err = system.query::<u32>().err().unwrap();
    assert!(err.to_string().contains("not registered to store events"));

    let err = system.query_blocking::<u32>().err().unwrap();
    assert!(err.to_string().contains("not registered to store events"));
}

#[test]
fn test_enable_disable_unregistered() {
    let system = EventBackend::default();

    let err = system.disable::<u32>().unwrap_err();
    assert!(err.to_string().contains("Unregistered event type"));

    let err = system.enable::<u32>().unwrap_err();
    assert!(err.to_string().contains("Unregistered event type"));
}

#[test]
fn test_error_debug_names_type() {
    let system = EventBackend::default();

    let err = system.query::<u32>().err().unwrap();
    let debug = format!("{err:?}");

    assert!(debug.contains("UnregisteredEventType"));
    assert!(debug.contains("u32"));
}
