# Changelog

## 0.2.1

### Added

- `EventBackend` implements `anythingy::HeapSize`, reporting the heap memory its registrations, listeners and stored
  events have allocated.
- `UnblockingQuery` implements `ExactSizeIterator`.
- `keywords` and `categories` package metadata.
- This changelog.

### Fixed

- README: typos, the broken licence link, and missing documentation of slot types and memory usage.

### Changed

- **Breaking:** `EventBackend::register_store` no longer returns a `Result`, and `EventBackend::register_listener`
  returns the number of registered listeners directly instead of a `Result` of it.
- **Breaking:** events are no longer limited in size. `EventBackend` lost its `const EVENT_SIZE` parameter (and
  `UnblockingQuery` its matching one), and any type that is `Send + RefUnwindSafe + 'static` can be used as an event.
- `EventBackend::query` takes the stored batch out of the event system up front. Buffers of consumed batches are reused
  for the next batch instead of being allocated again.
- `SlotType::All` and `SlotType::AllFilter` store events in a lock-free `anythingy::EventQueue`, so producer threads
  no longer wait for each other. The order of events is only guaranteed per thread, not between threads.
- `SlotType::Last` is lock-free (backed by `anythingy::AtomicSlot`).
- Updated `anythingy` from `0.1.2` to `0.3.4`.
- Updated to Rust edition 2024.

### Removed

- **Breaking:** `EventBackend::query_blocking`, together with the `Query` type it returned. Use
  `EventBackend::query`.
