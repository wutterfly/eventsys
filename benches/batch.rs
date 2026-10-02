use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use eventsys::{EventBackend, SlotType};
use std::hint::black_box;

type Backend = EventBackend;

/// Cost of storing a single event for the slot types with different code paths.
fn events_slot_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_slots");

    let cases: [(&str, SlotType<u64>); 4] = [
        ("all", SlotType::All),
        ("first", SlotType::First),
        ("last", SlotType::Last),
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

                // consume regularly, so slots that keep everything do not grow without bound
                if i & 1023 == 0 {
                    drop(events.consume::<u64>().unwrap());
                }
            });
        });
    }
}

/// Cost of polling an empty slot (nothing stored), for each slot type.
fn events_idle_poll(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_idle_poll");

    let cases: [(&str, SlotType<u64>); 4] = [
        ("all", SlotType::All),
        ("first", SlotType::First),
        ("last", SlotType::Last),
        ("all_filter", SlotType::AllFilter(|new| new % 2 == 0)),
    ];

    for (name, typ) in cases {
        let mut events = Backend::new();
        events.register_store::<u64>(typ);

        group.bench_function(name, |b| {
            b.iter(|| black_box(events.consume::<u64>().unwrap().count()));
        });
    }
}

/// Cost of pushing under real concurrent contention: several threads pushing the same slot at once, for each
/// slot type's code path.
///
/// `batch_slots` alone cannot show whether `Last`'s lock-free push actually pays off: its single-threaded
/// number already includes its own overhead (an allocation on the first two pushes ever, or whenever two pushes
/// race for the recycled spare box), but says nothing about the lock it avoids, since nothing there contends
/// for a lock in the first place. Running the same pushes from several threads at once isolates that.
fn events_slot_types_contended(c: &mut Criterion) {
    const PRODUCERS: u64 = 4;
    const PER_PRODUCER: u64 = 2_000;

    let mut group = c.benchmark_group("batch_slots_contended");
    group.sample_size(20);
    group.throughput(Throughput::Elements(PRODUCERS * PER_PRODUCER));

    let cases: [(&str, SlotType<u64>); 4] = [
        ("all", SlotType::All),
        ("first", SlotType::First),
        ("last", SlotType::Last),
        ("all_filter", SlotType::AllFilter(|new| new % 2 == 0)),
    ];

    for (name, typ) in cases {
        let mut events = Backend::new();
        events.register_store::<u64>(typ);

        group.bench_function(name, |b| {
            b.iter(|| {
                std::thread::scope(|s| {
                    for _ in 0..PRODUCERS {
                        s.spawn(|| {
                            for i in 0..PER_PRODUCER {
                                events.new_event::<u64>(black_box(i)).unwrap();
                            }
                        });
                    }
                });

                // consume between iterations, so slots that keep everything do not grow without bound
                drop(events.consume::<u64>().unwrap());
            });
        });
    }
}

criterion_group!(
    benches,
    events_slot_types,
    events_idle_poll,
    events_slot_types_contended
);
criterion_main!(benches);
