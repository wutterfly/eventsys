use criterion::{Criterion, criterion_group, criterion_main};
use eventsys::{EventBackend, SlotType};
use std::hint::black_box;

type Backend = EventBackend;

/// Cost of storing a single event for the slot types with different code paths.
fn events_slot_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_slots");

    let cases: [(&str, SlotType<u64>); 3] = [
        ("all", SlotType::All),
        ("first", SlotType::First),
        ("all_filter", SlotType::AllFilter(|new| new % 2 == 0)),
    ];

    for (name, typ) in cases {
        let mut events = Backend::new();
        events.register_store::<u64>(typ);

        group.bench_function(name, |b| {
            let mut i = 0u64;

            b.iter(|| {
                i = i.wrapping_add(1);
                events.new_event::<u64>(black_box(i)).unwrap();

                // drain regularly, so slots that keep everything do not grow without bound
                if i & 1023 == 0 {
                    drop(events.query_blocking::<u64>().unwrap());
                }
            });
        });
    }
}

criterion_group!(benches, events_slot_types);
criterion_main!(benches);
