use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use eventsys::{EventBackend, SlotType};

#[test]
fn test_listener_and_store_on_same_type() {
    let mut system = EventBackend::default();

    let seen = Arc::new(Mutex::new(Vec::<u32>::new()));
    {
        let seen = seen.clone();
        system.register_listener::<u32>(move |event| seen.lock().unwrap().push(*event));
    }
    system.register_store::<u32>(SlotType::All);

    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    // listener was called immediately
    assert_eq!(&*seen.lock().unwrap(), &[1, 2]);

    // event was stored for later
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn test_store_registered_before_listener() {
    let mut system = EventBackend::default();

    system.register_store::<u32>(SlotType::All);

    let count = Arc::new(AtomicUsize::new(0));
    {
        let count = count.clone();
        let n = system.register_listener::<u32>(move |_| _ = count.fetch_add(1, Ordering::Relaxed));
        assert_eq!(n, 1);
    }

    system.new_event::<u32>(1).unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [1]);
}

#[test]
fn test_listener_count_is_per_type() {
    let mut system = EventBackend::default();

    assert_eq!(system.register_listener::<u32>(|_| {}), 1);
    assert_eq!(system.register_listener::<u32>(|_| {}), 2);
    assert_eq!(system.register_listener::<i32>(|_| {}), 1);
    assert_eq!(system.register_listener::<u32>(|_| {}), 3);
}

#[test]
fn test_listeners_only_receive_matching_type() {
    let mut system = EventBackend::default();

    let u32_count = Arc::new(AtomicUsize::new(0));
    let i32_count = Arc::new(AtomicUsize::new(0));
    {
        let c = u32_count.clone();
        system.register_listener::<u32>(move |_| _ = c.fetch_add(1, Ordering::Relaxed));
    }
    {
        let c = i32_count.clone();
        system.register_listener::<i32>(move |_| _ = c.fetch_add(1, Ordering::Relaxed));
    }

    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();
    system.new_event::<i32>(3).unwrap();

    assert_eq!(u32_count.load(Ordering::Relaxed), 2);
    assert_eq!(i32_count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_panicking_listener_does_not_break_dispatch() {
    let mut system = EventBackend::default();

    let count = Arc::new(AtomicUsize::new(0));

    system.register_listener::<u32>(|_| panic!("listener panic (expected in test)"));
    {
        let count = count.clone();
        system.register_listener::<u32>(move |_| _ = count.fetch_add(1, Ordering::Relaxed));
    }
    system.register_store::<u32>(SlotType::All);

    system.new_event::<u32>(1).unwrap();
    system.new_event::<u32>(2).unwrap();

    // listeners after the panicking one are still called, events are still stored
    assert_eq!(count.load(Ordering::Relaxed), 2);
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn test_disable_skips_listeners_and_store() {
    let mut system = EventBackend::default();

    let count = Arc::new(AtomicUsize::new(0));
    {
        let count = count.clone();
        system.register_listener::<u32>(move |_| _ = count.fetch_add(1, Ordering::Relaxed));
    }
    system.register_store::<u32>(SlotType::All);

    system.disable::<u32>().unwrap();
    // disabled events are not an error
    system.new_event::<u32>(1).unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 0);
    assert_eq!(system.consume::<u32>().unwrap().len(), 0);

    system.enable::<u32>().unwrap();
    system.new_event::<u32>(2).unwrap();

    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [2]);
}

#[test]
fn test_disable_all_enable_all() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<i32>(SlotType::All);

    system.disable_all();

    system.new_event::<u32>(1).unwrap();
    system.new_event::<i32>(-1).unwrap();

    assert_eq!(system.consume::<u32>().unwrap().len(), 0);
    assert_eq!(system.consume::<i32>().unwrap().len(), 0);

    system.enable_all();

    system.new_event::<u32>(2).unwrap();
    system.new_event::<i32>(-2).unwrap();

    assert_eq!(system.consume::<u32>().unwrap().collect::<Vec<_>>(), [2]);
    assert_eq!(system.consume::<i32>().unwrap().collect::<Vec<_>>(), [-2]);
}

#[test]
fn test_disable_only_affects_one_type() {
    let mut system = EventBackend::default();
    system.register_store::<u32>(SlotType::All);
    system.register_store::<i32>(SlotType::All);

    system.disable::<u32>().unwrap();

    system.new_event::<u32>(1).unwrap();
    system.new_event::<i32>(-1).unwrap();

    assert_eq!(system.consume::<u32>().unwrap().len(), 0);
    assert_eq!(system.consume::<i32>().unwrap().collect::<Vec<_>>(), [-1]);
}

#[test]
fn test_disable_all_on_empty_backend() {
    let system = EventBackend::default();

    system.disable_all();
    system.enable_all();
}

#[test]
fn test_threads_trigger_events() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 1_000;

    let mut system = EventBackend::new();

    let count = Arc::new(AtomicUsize::new(0));
    {
        let count = count.clone();
        system.register_listener::<usize>(move |_| _ = count.fetch_add(1, Ordering::Relaxed));
    }
    system.register_store::<usize>(SlotType::All);

    std::thread::scope(|s| {
        for t in 0..THREADS {
            let system = &system;
            s.spawn(move || {
                for i in 0..PER_THREAD {
                    system.new_event::<usize>(t * PER_THREAD + i).unwrap();
                }
            });
        }
    });

    assert_eq!(count.load(Ordering::Relaxed), THREADS * PER_THREAD);

    // every event arrived exactly once
    let mut events = system.consume::<usize>().unwrap().collect::<Vec<_>>();
    events.sort_unstable();
    assert_eq!(events, (0..THREADS * PER_THREAD).collect::<Vec<_>>());
}

#[test]
fn test_threads_trigger_while_consuming() {
    const TOTAL: usize = 10_000;

    let mut system = EventBackend::new();
    system.register_store::<usize>(SlotType::All);

    let received = std::thread::scope(|s| {
        let producer = s.spawn(|| {
            for i in 0..TOTAL {
                system.new_event::<usize>(i).unwrap();
            }
        });

        // consume batches while events are still being produced
        let mut received = Vec::with_capacity(TOTAL);
        while !producer.is_finished() {
            received.extend(system.consume::<usize>().unwrap());
        }
        producer.join().unwrap();

        // final batch
        received.extend(system.consume::<usize>().unwrap());
        received
    });

    // no event lost, duplicated or reordered
    assert_eq!(received, (0..TOTAL).collect::<Vec<_>>());
}
