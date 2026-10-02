# Changelog

## 0.2.1

### Added

- `EventBackend` implements `anythingy::HeapSize`, reporting the heap memory its registrations, listeners and stored
  events have allocated.
- `EventBackend::observe` and `EventBackend::reset`, for observing the same events in several places. The first
  `observe` of a type takes its events out of the event system into a recycled buffer, and every `observe` after that
  returns the same events to any number of observers, until `reset` starts a new round. Observing needs no mutable
  access and no event type has to be known up front.
- `Observed`, an iterator over the borrowed events that `EventBackend::observe` returns. It has the same API as
  `Consumed`: `len`, `is_empty` and `as_slice`. Neither is `Clone` or `Copy`.
- `Consumed` (the iterator returned by `EventBackend::consume`) implements `ExactSizeIterator`, has `as_slice` for the
  events that were not consumed yet, and is exported next to `Observed`.
- `EventError`, `RawErr`, `Value` and `NoValue` are exported, so errors can be named in signatures, and what went wrong can
  be matched with `EventError::raw_err()`. `RawErr` is a plain `Copy` enum that carries the name of the event type, and
  more kinds can be added, so it is `#[non_exhaustive]`.
- `keywords` and `categories` package metadata, and `rust-version`: the minimum supported Rust version is `1.88`.
- This changelog.

### Fixed

- README: typos, the broken licence link, and missing documentation of slot types and memory usage.
- The `Debug` output of an `EventError` named the error `BoxedEventError`, and the kind of error for an event type
  without a store `RegisteredWithListener`: they are `EventError` and `RegisteredWithoutStore`. The messages of both
  kinds of error name the event type now.

### Changed

- **Breaking:** `EventBackend::register_store` no longer returns a `Result`, and `EventBackend::register_listener`
  returns the number of registered listeners directly instead of a `Result` of it.
- **Breaking:** events are no longer limited in size. `EventBackend` lost its `const EVENT_SIZE` parameter (and
  `Consumed` its matching one), and any type that is `Send + RefUnwindSafe + 'static` can be used as an event.
- **Breaking:** `EventBackend::query` is now `EventBackend::consume`, and the `UnblockingQuery` it returned is now
  `Consumed`. It takes the stored batch out of the event system up front, and buffers of consumed batches are reused
  for the next batch instead of being allocated again.
- **Breaking:** `EventBackend::cleanup` is now `EventBackend::clear`. It drops all stored events, but keeps what was
  allocated for them and leaves the registered listeners alone, where `cleanup` freed the memory and dropped the
  listeners.
- `SlotType::All` and `SlotType::AllFilter` store events in a lock-free `anythingy::EventQueue`, so producer threads
  no longer wait for each other. The order of events is only guaranteed per thread, not between threads.
- `SlotType::Last` is lock-free (backed by `anythingy::AtomicSlot`).
- Finding an event type no longer gets slower with every registered type: a few are found by comparing, and all of
  them by hashing once there are more. A lookup among 256 types takes about 3.5 ns instead of about 100 ns.
- Updated `anythingy` from `0.1.2` to `0.3.5`.
- Updated to Rust edition 2024.

### Removed

- **Breaking:** `EventBackend::query_blocking`, together with the `Query` type it returned. Use
  `EventBackend::consume`.
