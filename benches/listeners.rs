use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use eventsys::EventBackend;
use std::hint::black_box;

type Backend = EventBackend<16>;

#[inline(never)]
fn raw(event: f64) {
    _ = black_box(event);
}

/// Cost of dispatching to a growing number of listeners of the same type.
fn events_listener_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("listeners_count");

    for count in [1usize, 16] {
        let mut events = Backend::new();

        for _ in 0..count {
            events
                .register_listener::<f64>(|event| raw(*event))
                .unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| events.new_event::<f64>(black_box(64.0)).unwrap());
        });
    }
}

criterion_group!(benches, events_listener_count);
criterion_main!(benches);
