use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eventsys::EventBackend;

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

    let listener = |event: &f64| _ = black_box(raw(*event));
    events.register_listener::<f64>(listener).unwrap();

    events
}

fn events_listener(c: &mut Criterion) {
    let mut group = c.benchmark_group("listeners");

    let events = create_backend();

    group.bench_function("direct function", |b| b.iter(|| black_box(raw(64.0))));

    group.bench_function("event", |b| {
        b.iter(|| black_box(new_event_bench(&events, 64.0)))
    });

    group.bench_function("event unregistered", |b| {
        b.iter(|| black_box(new_event_unregistered(&events, 64.0)))
    });
}

criterion_group!(benches, events_listener);
criterion_main!(benches);
