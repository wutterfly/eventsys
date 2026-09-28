//! Benchmarks that resemble how the crate is used in an application.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use eventsys::{EventBackend, SlotType};
use std::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering},
};

type Backend = EventBackend;

// ############################
// Frame loop
// ############################

struct MouseMove {
    x: f32,
    y: f32,
}

struct MouseButton {
    button: u8,
    pressed: bool,
}

struct Key {
    code: u32,
    pressed: bool,
}

struct Scroll {
    dx: f32,
    dy: f32,
}

struct Resize {
    w: u32,
    h: u32,
}

struct Redraw {
    id: u32,
    dirty: bool,
}

struct Telemetry(u64);

/// Event system of an application with an input loop.
///
/// Only the newest mouse position and window size matter, key and button events must not get lost.
fn frame_backend() -> Backend {
    let mut events = Backend::new();

    events.register_store::<MouseMove>(SlotType::Last).unwrap();
    events.register_store::<MouseButton>(SlotType::All).unwrap();
    events.register_store::<Key>(SlotType::All).unwrap();
    events.register_store::<Scroll>(SlotType::Max(8)).unwrap();
    events.register_store::<Resize>(SlotType::Last).unwrap();
    events
        .register_store::<Redraw>(SlotType::AllFilter(|redraw| redraw.dirty))
        .unwrap();

    // statistics are handled immediately
    events
        .register_listener::<Telemetry>(|event| _ = black_box(event.0))
        .unwrap();

    events
}

/// Events, that arrive during one frame.
fn trigger_frame(events: &Backend, frame: u32) {
    for i in 0..16 {
        events
            .new_event(MouseMove {
                x: i as f32,
                y: frame as f32,
            })
            .unwrap();
    }

    for i in 0..2 {
        events
            .new_event(MouseButton {
                button: i,
                pressed: frame % 2 == 0,
            })
            .unwrap();
    }

    for i in 0..6 {
        events
            .new_event(Key {
                code: frame + i,
                pressed: true,
            })
            .unwrap();
    }

    for i in 0..3 {
        events
            .new_event(Scroll {
                dx: i as f32,
                dy: 1.0,
            })
            .unwrap();
    }

    if frame % 64 == 0 {
        events
            .new_event(Resize {
                w: 1920,
                h: 1080 + frame,
            })
            .unwrap();
    }

    for i in 0..4 {
        events
            .new_event(Redraw {
                id: i,
                dirty: i % 2 == 0,
            })
            .unwrap();
    }

    for i in 0..8 {
        events.new_event(Telemetry(i)).unwrap();
    }
}

/// Handles all stored events of a frame.
macro_rules! consume_frame {
    ($events:expr, $query:ident) => {{
        let events = $events;
        let mut acc = 0u64;

        for e in events.$query::<MouseMove>().unwrap() {
            acc += (e.x + e.y) as u64;
        }
        for e in events.$query::<MouseButton>().unwrap() {
            acc += u64::from(e.button) + u64::from(e.pressed);
        }
        for e in events.$query::<Key>().unwrap() {
            acc += u64::from(e.code) + u64::from(e.pressed);
        }
        for e in events.$query::<Scroll>().unwrap() {
            acc += (e.dx + e.dy) as u64;
        }
        for e in events.$query::<Resize>().unwrap() {
            acc += u64::from(e.w) + u64::from(e.h);
        }
        for e in events.$query::<Redraw>().unwrap() {
            acc += u64::from(e.id);
        }

        black_box(acc)
    }};
}

/// One frame: events arrive, then all stored events get handled.
fn frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame");

    let events = frame_backend();
    let mut n = 0u32;
    group.bench_function("busy/query", |b| {
        b.iter(|| {
            n = n.wrapping_add(1);
            trigger_frame(&events, n);
            consume_frame!(&events, query)
        });
    });

    let events = frame_backend();
    let mut n = 0u32;
    group.bench_function("busy/query_blocking", |b| {
        b.iter(|| {
            n = n.wrapping_add(1);
            trigger_frame(&events, n);
            consume_frame!(&events, query_blocking)
        });
    });

    // most frames have nothing to handle
    let events = frame_backend();
    group.bench_function("idle/query", |b| {
        b.iter(|| consume_frame!(&events, query));
    });
}

// ############################
// Event bus
// ############################

/// Distinct event type for every `N`.
struct Msg<const N: usize>(u32);

macro_rules! bus {
    ($($n:literal),* $(,)?) => {
        /// Every event type has 2 listeners.
        fn bus_backend() -> Backend {
            let mut events = Backend::new();

            $(
                events
                    .register_listener::<Msg<$n>>(|event| _ = black_box(event.0))
                    .unwrap();
                events
                    .register_listener::<Msg<$n>>(|event| _ = black_box(event.0 + 1))
                    .unwrap();
            )*

            events
        }

        /// One event of the given event type.
        fn bus_fire(events: &Backend, kind: u32, value: u32) {
            match kind {
                $(
                    $n => events.new_event(Msg::<$n>(value)).unwrap(),
                )*
                _ => unreachable!(),
            }
        }
    };
}

bus!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

/// Application with many event types, all handled by listeners.
///
/// Event types arrive in a pseudo-random order (fixed seed), like in a real event stream. A fixed order would let the
/// branch predictor learn the whole sequence, which makes the result depend heavily on the code layout.
fn event_bus(c: &mut Criterion) {
    const EVENTS: u64 = 16;

    let mut group = c.benchmark_group("bus");

    let events = bus_backend();
    let mut rng = 0x2545_F491_u32;

    group.throughput(Throughput::Elements(EVENTS));
    group.bench_function("16 types, 2 listeners each", |b| {
        b.iter(|| {
            for _ in 0..EVENTS {
                // xorshift32
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;

                bus_fire(&events, (rng >> 8) & 15, black_box(7));
            }
        });
    });
}

// ############################
// Pipeline
// ############################

struct Sample(u64);

struct Tick(u64);

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Several threads produce events, while one thread handles them in batches.
fn pipeline(c: &mut Criterion) {
    const PRODUCERS: u64 = 4;
    const SAMPLES: u64 = 2_500;

    let mut group = c.benchmark_group("pipeline");
    group.sample_size(20);
    group.throughput(Throughput::Elements(PRODUCERS * SAMPLES));

    let mut events = Backend::new();
    events.register_store::<Sample>(SlotType::All).unwrap();
    // only the latest ticks matter, every tick also gets counted immediately
    events.register_store::<Tick>(SlotType::Max(64)).unwrap();
    events
        .register_listener::<Tick>(|tick| {
            _ = TICKS.fetch_add(black_box(tick.0) & 1, Ordering::Relaxed)
        })
        .unwrap();

    group.bench_function("4 producers, 1 consumer", |b| {
        b.iter(|| {
            std::thread::scope(|s| {
                for _ in 0..PRODUCERS {
                    s.spawn(|| {
                        for i in 0..SAMPLES {
                            events.new_event(Sample(i)).unwrap();

                            if i % 8 == 0 {
                                events.new_event(Tick(i)).unwrap();
                            }
                        }
                    });
                }

                // handle events in batches, until every sample arrived
                let mut received = 0u64;
                let mut acc = 0u64;
                while received < PRODUCERS * SAMPLES {
                    for sample in events.query::<Sample>().unwrap() {
                        acc = acc.wrapping_add(sample.0);
                        received += 1;
                    }

                    // give the producers time to make progress
                    std::thread::yield_now();
                }

                black_box(acc)
            })
        });
    });
}

criterion_group!(benches, frame, event_bus, pipeline);
criterion_main!(benches);
