use eventsys::EventBackend;

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
fn test_query_unregistered() {
    let system = EventBackend::default();

    let err = system.query::<u32>().err().unwrap();
    assert!(err.to_string().contains("Unregistered event type"));
}

#[test]
fn test_query_registered_without_store() {
    let mut system = EventBackend::default();
    system.register_listener::<u32>(|_| {});

    let err = system.query::<u32>().err().unwrap();
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
