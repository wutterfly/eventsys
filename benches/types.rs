use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use eventsys::{EventBackend, SlotType};
use std::hint::black_box;

type Backend = EventBackend;

/// Distinct event type for every `N`.
struct Msg<const N: usize>;

macro_rules! types {
    ($($n:literal),* $(,)?) => {
        /// Registers the first `count` event types.
        fn register(events: &mut Backend, count: usize) {
            $(
                if $n < count {
                    // `First` rejects every event after the first one without touching a lock, so what is left
                    // of a `new_event` is mostly finding the event type
                    events.register_store::<Msg<$n>>(SlotType::First);
                }
            )*
        }

        fn fire(events: &Backend, n: usize) {
            match n {
                $( $n => events.new_event(Msg::<$n>).unwrap(), )*
                _ => unreachable!(),
            }
        }

        fn consume_nothing(events: &Backend, n: usize) -> usize {
            match n {
                $( $n => events.consume::<Msg<$n>>().unwrap().count(), )*
                _ => unreachable!(),
            }
        }

        fn observe_again(events: &Backend, n: usize) -> usize {
            match n {
                $( $n => events.observe::<Msg<$n>>().unwrap().len(), )*
                _ => unreachable!(),
            }
        }
    };
}

types!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,
    98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116,
    117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
    136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154,
    155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173,
    174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192,
    193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211,
    212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230,
    231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249,
    250, 251, 252, 253, 254, 255
);

fn backend(count: usize) -> Backend {
    let mut events = Backend::new();
    register(&mut events, count);
    events
}

/// How the cost of finding an event type grows with the number of registered types: every operation scans the
/// registered types for the one it needs, so the type that was registered last is the worst case, and the one
/// registered first the best.
fn lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("types_lookup");

    for count in [1usize, 16, 64, 256] {
        let last = count - 1;

        let events = backend(count);
        group.bench_with_input(BenchmarkId::new("new_event/last", count), &count, |b, _| {
            b.iter(|| fire(&events, black_box(last)));
        });

        let events = backend(count);
        group.bench_with_input(
            BenchmarkId::new("new_event/first", count),
            &count,
            |b, _| {
                b.iter(|| fire(&events, black_box(0)));
            },
        );

        let events = backend(count);
        group.bench_with_input(
            BenchmarkId::new("consume_nothing/last", count),
            &count,
            |b, _| {
                b.iter(|| consume_nothing(&events, black_box(last)));
            },
        );

        // fetched once, so every observe afterwards only has to find the type, and read
        let events = backend(count);
        fire(&events, last);
        assert_eq!(observe_again(&events, last), 1);
        group.bench_with_input(
            BenchmarkId::new("observe_again/last", count),
            &count,
            |b, _| {
                b.iter(|| observe_again(&events, black_box(last)));
            },
        );
    }
}

/// How the cost of visiting every registered type (ending a round, or clearing) grows with their number.
fn visit_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("types_visit_all");

    for count in [1usize, 16, 64, 256] {
        let mut events = backend(count);
        group.bench_with_input(BenchmarkId::new("reset", count), &count, |b, _| {
            b.iter(|| events.reset());
        });

        let mut events = backend(count);
        group.bench_with_input(BenchmarkId::new("clear", count), &count, |b, _| {
            b.iter(|| events.clear());
        });
    }
}

criterion_group!(benches, lookup, visit_all);
criterion_main!(benches);
