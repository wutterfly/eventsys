use std::sync::{Arc, Mutex};

use eventsys::EventBackend;

#[test]
fn test_listeners() {
    let mut system = EventBackend::default();

    let state = Arc::new(Mutex::new(Vec::<u32>::new()));
    // Setup single event listener
    {
        let state_c = state.clone();
        let listener = move |event: &u32| {
            state_c.lock().unwrap().push(*event);
        };
        system.register_listener::<u32>(listener).unwrap();
    }

    let state_boxed = Arc::new(Mutex::new(Vec::<Box<str>>::new()));
    // Setup single event listener
    {
        let state_boxed_c = state_boxed.clone();
        let listener = move |event: &Box<str>| {
            state_boxed_c.lock().unwrap().push(event.clone());
        };
        system.register_listener::<Box<str>>(listener).unwrap();
    }

    // call  events
    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    // call boxed events
    system.new_event::<Box<str>>(Box::from("Hello")).unwrap();
    system.new_event::<Box<str>>(Box::from("World")).unwrap();

    // check events
    let unboxed = &*state.lock().unwrap();
    let boxed = &*state_boxed.lock().unwrap();

    assert_eq!(unboxed, &[1, 2]);
    assert_eq!(boxed, &[Box::from("Hello"), Box::from("World")]);
}

#[test]
fn test_listeners_multiple() {
    let mut system = EventBackend::default();

    let state = Arc::new(Mutex::new(Vec::<u32>::new()));
    // Setup single event listener
    {
        let state_c = state.clone();
        let listener = move |event: &u32| {
            state_c.lock().unwrap().push(event + 10);
        };
        let count = system.register_listener::<u32>(listener).unwrap();
        assert_eq!(count, 1);
    }

    // Setup single event listener
    {
        let state_c = state.clone();
        let listener = move |event: &u32| {
            state_c.lock().unwrap().push(event + 20);
        };
        let count = system.register_listener::<u32>(listener).unwrap();
        assert_eq!(count, 2);
    }

    // call  events
    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    // check events
    let unboxed = &*state.lock().unwrap();

    assert_eq!(unboxed, &[11, 21, 12, 22]);
}
