use eventsys::{EventBackend, EventError, NoValue, RawErr, Value};

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
fn test_consume_unregistered() {
    let system = EventBackend::default();

    let err = system.consume::<u32>().err().unwrap();
    assert!(err.to_string().contains("Unregistered event type"));
}

#[test]
fn test_consume_registered_without_store() {
    let mut system = EventBackend::default();
    system.register_listener::<u32>(|_| {});

    let err = system.consume::<u32>().err().unwrap();
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

    let err = system.consume::<u32>().err().unwrap();
    let debug = format!("{err:?}");

    assert!(debug.contains("UnregisteredEventType"));
    assert!(debug.contains("u32"));
}

#[test]
fn test_error_kind_can_be_matched() {
    let mut system = EventBackend::default();
    system.register_listener::<u64>(|_| {});

    // not registered at all
    let error = system.consume::<u32>().err().unwrap();
    let RawErr::UnregisteredEventType { type_name } = error.raw_err() else {
        panic!("{error}");
    };
    assert!(type_name.contains("u32"));

    // registered, but without a store
    let error = system.consume::<u64>().err().unwrap();
    let RawErr::RegisteredWithoutStore { type_name } = error.raw_err() else {
        panic!("{error}");
    };
    assert!(type_name.contains("u64"));
}

#[test]
fn test_error_kinds_can_be_compared_and_copied() {
    let system = EventBackend::default();

    let first = system.consume::<u32>().err().unwrap().raw_err();
    let second = system.consume::<u32>().err().unwrap().raw_err();
    let other = system.consume::<u64>().err().unwrap().raw_err();

    // `RawErr` is `Copy`, whatever the event type is
    let copy = first;
    assert_eq!(first, copy);
    assert_eq!(first, second);
    assert_ne!(first, other, "another type is another error");
}

#[test]
fn test_error_can_be_named_in_signatures() {
    fn trigger(system: &EventBackend, value: u32) -> Result<(), EventError<u32, Value>> {
        system.new_event(value)
    }

    fn consume(system: &EventBackend) -> Result<usize, EventError<u32, NoValue>> {
        Ok(system.consume::<u32>()?.len())
    }

    let system = EventBackend::default();
    assert_eq!(trigger(&system, 7).unwrap_err().into_inner(), 7);
    assert!(consume(&system).is_err());

    let mut system = EventBackend::default();
    system.register_store::<u32>(eventsys::SlotType::All);
    trigger(&system, 7).unwrap();
    assert_eq!(consume(&system).unwrap(), 1);
}

#[test]
fn test_error_can_be_passed_on_as_a_dyn_error() {
    fn consume(system: &EventBackend) -> Result<(), Box<dyn std::error::Error>> {
        system.consume::<u32>()?;
        Ok(())
    }

    let system = EventBackend::default();
    let error = consume(&system).unwrap_err();
    assert!(error.to_string().contains("Unregistered event type"));

    // the event is not part of the boxed error, and it can be sent between threads
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EventError<u32, NoValue>>();
    assert_send_sync::<RawErr>();
}

#[test]
fn test_error_messages_name_the_type() {
    let mut system = EventBackend::default();
    system.register_listener::<u64>(|_| {});

    let unregistered = system.consume::<u32>().err().unwrap().to_string();
    assert!(
        unregistered.contains("Unregistered event type"),
        "{unregistered}"
    );
    assert!(unregistered.contains("u32"), "{unregistered}");

    let without_store = system.consume::<u64>().err().unwrap().to_string();
    assert!(
        without_store.contains("not registered to store events"),
        "{without_store}"
    );
    assert!(without_store.contains("u64"), "{without_store}");
}

#[test]
fn test_error_debug_has_the_right_names() {
    let mut system = EventBackend::default();
    system.register_listener::<u64>(|_| {});

    let debug = format!("{:?}", system.consume::<u32>().err().unwrap());
    assert!(debug.starts_with("EventError"), "{debug}");

    let debug = format!("{:?}", system.consume::<u64>().err().unwrap());
    assert!(debug.contains("RegisteredWithoutStore"), "{debug}");
}
