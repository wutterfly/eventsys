use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eventsys::{EventBackend, SlotType};

type Backend = EventBackend<16>;

#[inline(never)]
fn raw(event: f64) {
    _ = black_box(event);
}

fn new_event_bench(events: &Backend, value: f64) {
    events.new_event::<f64>(value).unwrap();
}

fn new_event_unregistered(events: &Backend, value: f32) {
    _ = events.new_event::<f32>(value).unwrap_err();
}

fn create_backend() -> Backend {
    let mut events = Backend::new();

    events.register_store::<f64>(SlotType::Max(10_000)).unwrap();

    // preallocate some memory
    for _ in 0..10_000 {
        events.new_event::<f64>(0.0).unwrap();
    }

    // clear buffer
    events.query_blocking::<f64>().unwrap();

    events
}

fn events_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch");

    let events = create_backend();

    group.bench_function("direct function", |b| b.iter(|| black_box(raw(64.0))));

    group.bench_function("event", |b| {
        b.iter(|| black_box(new_event_bench(&events, 64.0)))
    });

    // clear buffer
    events.query_blocking::<f64>().unwrap();

    group.bench_function("event unregistered", |b| {
        b.iter(|| black_box(new_event_unregistered(&events, 64.0)))
    });

    // clear buffer
    events.query_blocking::<f64>().unwrap();
}

criterion_group!(benches, events_batch);
criterion_main!(benches);
