use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use eventsys::{EventBackend, SlotType};
use std::hint::black_box;

type Backend = EventBackend;

fn trigger(events: &Backend, count: u64) {
    for i in 0..count {
        events.new_event::<u64>(black_box(i)).unwrap();
    }
}

/// Cost of getting a batch of events to a single place: consuming them, or observing them (which has to copy them
/// into a buffer of its own, and has to be reset afterwards).
///
/// Observing is what makes it possible to hand the same events to several places, so what is measured here is
/// what that costs, compared to consuming, when there is only one.
fn consume_or_observe(c: &mut Criterion) {
    let mut group = c.benchmark_group("observe_vs_consume");

    for count in [100u64, 1_000] {
        group.throughput(Throughput::Elements(count));

        let mut events = Backend::new();
        events.register_store::<u64>(SlotType::All);

        group.bench_with_input(BenchmarkId::new("consume", count), &count, |b, &count| {
            b.iter(|| {
                trigger(&events, count);

                black_box(events.consume::<u64>().unwrap().sum::<u64>())
            });
        });

        group.bench_with_input(BenchmarkId::new("observe", count), &count, |b, &count| {
            b.iter(|| {
                trigger(&events, count);

                let sum = events.observe::<u64>().unwrap().sum::<u64>();
                events.reset();

                black_box(sum)
            });
        });

        // the second observer only reads: this is what every further observer costs
        group.bench_with_input(
            BenchmarkId::new("observe_two_observers", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    trigger(&events, count);

                    let first = events.observe::<u64>().unwrap().sum::<u64>();
                    let second = events.observe::<u64>().unwrap().sum::<u64>();
                    events.reset();

                    black_box((first, second))
                });
            },
        );
    }
}

/// Cost of observing events that were already fetched this round, which is what every observer but the first does:
/// finding the registration, and reading the buffer.
fn observe_again(c: &mut Criterion) {
    let mut group = c.benchmark_group("observe_again");

    for count in [1u64, 1_000] {
        group.throughput(Throughput::Elements(count));

        let mut events = Backend::new();
        events.register_store::<u64>(SlotType::All);
        trigger(&events, count);
        // fetches the events
        assert_eq!(events.observe::<u64>().unwrap().len(), count as usize);

        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| black_box(events.observe::<u64>().unwrap().sum::<u64>()));
        });
    }
}

/// Cost of a whole round, for every slot type's code path: events arrive, get observed, and are reset.
fn observe_slot_types(c: &mut Criterion) {
    const EVENTS: u64 = 64;

    let mut group = c.benchmark_group("observe_slots");
    group.throughput(Throughput::Elements(EVENTS));

    let cases: [(&str, SlotType<u64>); 6] = [
        ("all", SlotType::All),
        ("all_filter", SlotType::AllFilter(|new| new % 2 == 0)),
        ("max", SlotType::Max(32)),
        ("first", SlotType::First),
        ("last", SlotType::Last),
        ("cmp", SlotType::Cmp(|current, new| new > current)),
    ];

    for (name, typ) in cases {
        let mut events = Backend::new();
        events.register_store::<u64>(typ);

        group.bench_function(name, |b| {
            b.iter(|| {
                trigger(&events, EVENTS);

                let sum = events.observe::<u64>().unwrap().sum::<u64>();
                events.reset();

                black_box(sum)
            });
        });
    }
}

/// Distinct event type for every `N`.
struct Msg<const N: usize>(u64);

macro_rules! many_types {
    ($($n:literal),* $(,)?) => {
        /// Every event type is stored, and has one event.
        fn many_types_backend() -> Backend {
            let mut events = Backend::new();

            $(
                events.register_store::<Msg<$n>>(SlotType::Last);
            )*

            events
        }

        /// Every event type gets one event, and gets observed.
        fn many_types_round(events: &Backend) -> u64 {
            let mut sum = 0;

            $(
                events.new_event(Msg::<$n>(black_box($n))).unwrap();
                sum += events.observe::<Msg<$n>>().unwrap().map(|e| e.0).sum::<u64>();
            )*

            sum
        }
    };
}

many_types!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

/// Cost of ending a round (and of clearing), which has to visit every registered event type, whether it was
/// observed or not.
fn reset_and_clear(c: &mut Criterion) {
    let mut group = c.benchmark_group("observe_reset");

    // nothing was observed: the cost of visiting 16 registrations
    let mut events = many_types_backend();
    group.bench_function("reset/16 types, nothing observed", |b| {
        b.iter(|| events.reset());
    });

    // everything was observed: the cost of dropping the observed events of 16 registrations
    let mut events = many_types_backend();
    group.bench_function("round/16 types, all observed", |b| {
        b.iter(|| {
            let sum = many_types_round(&events);
            events.reset();

            black_box(sum)
        });
    });

    let mut events = many_types_backend();
    group.bench_function("clear/16 types, nothing stored", |b| {
        b.iter(|| events.clear());
    });
}

/// Cost of observing from several threads at once, while a round is in progress: the first call fetches the events,
/// every other one just reads them, so every observer should take as long as a single one would. The work of every
/// observer is the same, so if observing scales, all of these take about the same time (plus the thread start-up,
/// which is part of every number here and grows with the number of observers).
fn observe_contended(c: &mut Criterion) {
    const EVENTS: u64 = 100;
    const READS: u64 = 500;

    let mut group = c.benchmark_group("observe_contended");
    group.sample_size(20);

    let mut events = Backend::new();
    events.register_store::<u64>(SlotType::All);

    for observers in [1u64, 2, 4] {
        group.throughput(Throughput::Elements(observers * READS * EVENTS));

        group.bench_with_input(
            BenchmarkId::new("observers", observers),
            &observers,
            |b, &observers| {
                b.iter(|| {
                    trigger(&events, EVENTS);

                    std::thread::scope(|s| {
                        for _ in 0..observers {
                            s.spawn(|| {
                                let mut sum = 0;
                                for _ in 0..READS {
                                    sum += events.observe::<u64>().unwrap().sum::<u64>();
                                }

                                black_box(sum)
                            });
                        }
                    });

                    events.reset();
                });
            },
        );
    }

    // the same thread start-up, without observing anything: what the numbers above include
    group.bench_function("thread start-up only, 4 threads", |b| {
        b.iter(|| {
            std::thread::scope(|s| {
                for _ in 0..4 {
                    s.spawn(|| black_box(0u64));
                }
            });
        });
    });
}

criterion_group!(
    benches,
    consume_or_observe,
    observe_again,
    observe_slot_types,
    reset_and_clear,
    observe_contended
);
criterion_main!(benches);
