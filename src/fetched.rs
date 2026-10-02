use std::{
    any::Any,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use anythingy::{AtomicRefCell, HeapSize};

/// A buffer of fetched events, with its type erased.
///
/// Erasing the type is what keeps [`Fetched`] (and so every registration that holds one) `Sync` for events that are
/// only `Send`: `Vec<T>` is `Sync` only for `T: Sync`, but only observing the events needs that, and that is checked
/// where they are observed, in [`Fetched::observe`].
trait Buffer: Any + Send + Sync + HeapSize {
    /// Drops all events, but keeps the capacity.
    fn discard(&mut self);
}

impl<T: Send + Sync + 'static> Buffer for Vec<T> {
    #[inline]
    fn discard(&mut self) {
        self.clear();
    }
}

/// The events behind a `Buffer`, which is always a `Vec<T>` for the `T` that is asked for: a `Fetched` belongs to
/// the registration of exactly one type.
#[inline]
fn events<T: 'static>(buffer: &dyn Buffer) -> &[T] {
    (buffer as &dyn Any)
        .downcast_ref::<Vec<T>>()
        .unwrap_or_else(|| unreachable!("a `Fetched` only ever holds buffers of its own type"))
}

#[inline]
fn events_mut<T: 'static>(buffer: &mut dyn Buffer) -> &mut Vec<T> {
    (buffer as &mut dyn Any)
        .downcast_mut::<Vec<T>>()
        .unwrap_or_else(|| unreachable!("a `Fetched` only ever holds buffers of its own type"))
}

/// The events of one registered type, as they were when they were first asked for.
///
/// Filled lazily, on the first [`Fetched::observe`] after the last [`Fetched::reset`]; every `observe` after that
/// returns the same events, without any lock or atomic read-modify-write. Resetting needs mutable access. The buffer
/// stays where it is and keeps its capacity, so after a few rounds nothing is allocated.
///
/// `filled` decides who may touch `buffer`, which is why `buffer` is only ever borrowed without the cell's own
/// checks (they would cost every reader an atomic read-modify-write on a shared counter, for nothing):
/// - Before `filled` is set, only the one closure of [`OnceLock::get_or_init`] that is running may touch `buffer`.
/// - Once `filled` is set, `buffer` is only read, by any number of threads, until `reset`.
/// - `reset` needs `&mut self`, so nothing can be reading while it runs.
pub struct Fetched {
    /// Set, once `buffer` holds the events of this round.
    filled: OnceLock<()>,

    /// Created by the first fill, for the type that is asked for, and kept from then on.
    buffer: AtomicRefCell<Option<Box<dyn Buffer>>>,

    /// What `buffer` has allocated, as of the last fill. Kept next to it, so counting does not have to touch
    /// `buffer`, which may be filled by another thread at that very moment.
    heap: AtomicUsize,
}

impl Fetched {
    #[inline]
    pub const fn new() -> Self {
        Self {
            filled: OnceLock::new(),
            buffer: AtomicRefCell::new(None),
            heap: AtomicUsize::new(0),
        }
    }

    /// Returns the events of this round. The first call of a round lets `fill` put them into the buffer, any other
    /// call (even a concurrent one, which waits for the first) gets the same events.
    ///
    /// `T` has to be the same type on every call.
    #[inline]
    pub fn observe<T>(&self, fill: impl FnOnce(&mut Vec<T>)) -> &[T]
    where
        T: Send + Sync + 'static,
    {
        self.filled.get_or_init(|| {
            // SAFETY: `get_or_init` runs this closure on one thread at a time, and only while `filled` is not set.
            // Nobody else has a reference to `buffer` then: readers only borrow it after `get_or_init` returned,
            // which is after `filled` was set, and `reset` (the only way to unset it) needs `&mut self`, so it
            // can not run while any reader or this closure holds a reference derived from `&self`. If an earlier
            // fill panicked, its reference is gone with it, and `filled` was left unset.
            let slot = unsafe { self.buffer.borrow_mut_unchecked() };
            let buffer = slot.get_or_insert_with(|| Box::new(Vec::<T>::new()));

            let events = events_mut::<T>(&mut **buffer);
            // an earlier fill may have panicked half way
            events.clear();
            fill(events);

            let buffer: &dyn Buffer = &**buffer;
            self.heap
                .store(size_of_val(buffer) + buffer.heap_size(), Ordering::Relaxed);
        });

        // SAFETY: `filled` is set, so the fill is done and its exclusive reference is gone (`get_or_init` only
        // returns after the closure did, and its writes are visible here). Nothing changes `buffer` again until
        // `reset`, which needs `&mut self`, so only shared references to it exist for as long as the returned slice
        // can be used.
        let slot = unsafe { self.buffer.borrow_unchecked() };

        events::<T>(
            slot.as_deref().unwrap_or_else(|| {
                unreachable!("`filled` is only set after the buffer was created")
            }),
        )
    }

    /// Drops the events of this round, so the next `observe` fills the buffer again. The buffer keeps its capacity.
    #[inline]
    pub fn reset(&mut self) {
        if self.filled.take().is_some()
            && let Some(buffer) = self.buffer.get_mut()
        {
            buffer.discard();
        }
    }

    /// Counts the buffer, as it was after the last fill.
    #[inline]
    pub fn heap_size(&self) -> usize {
        self.heap.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::Fetched;

    #[test]
    fn test_fetched_fills_once_per_round() {
        let fetched = Fetched::new();
        let fills = AtomicUsize::new(0);

        for _ in 0..3 {
            let events = fetched.observe::<u32>(|buffer| {
                fills.fetch_add(1, Ordering::Relaxed);
                buffer.extend([1, 2, 3]);
            });
            assert_eq!(events, [1, 2, 3]);
        }

        assert_eq!(fills.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_fetched_threads_that_ask_first_at_the_same_time_fill_once() {
        let fetched = Fetched::new();
        let fills = AtomicUsize::new(0);

        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    let events = fetched.observe::<u32>(|buffer| {
                        fills.fetch_add(1, Ordering::Relaxed);
                        buffer.extend(0..100);
                    });
                    assert_eq!(events, (0..100).collect::<Vec<_>>());
                });
            }
        });

        assert_eq!(fills.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_fetched_reset_fills_again_in_the_same_buffer() {
        let mut fetched = Fetched::new();

        let first = fetched
            .observe::<u32>(|buffer| buffer.extend(0..1000))
            .as_ptr();
        fetched.reset();

        // the same allocation is used again, nothing new is allocated
        let second = fetched.observe::<u32>(|buffer| buffer.extend(0..1000));
        assert_eq!(second.as_ptr(), first);
        assert_eq!(second.len(), 1000);
    }

    #[test]
    fn test_fetched_reset_drops_the_events() {
        let mut fetched = Fetched::new();
        let probe = Arc::new(());

        fetched.observe::<Arc<()>>(|buffer| buffer.push(probe.clone()));
        assert_eq!(Arc::strong_count(&probe), 2);

        fetched.reset();
        assert_eq!(Arc::strong_count(&probe), 1);

        // resetting without anything fetched is fine
        fetched.reset();
        fetched.reset();
    }

    #[test]
    fn test_fetched_panicking_fill_can_be_retried() {
        let fetched = Fetched::new();

        let result = catch_unwind(AssertUnwindSafe(|| {
            fetched.observe::<u32>(|buffer| {
                buffer.push(1);
                panic!("fill failed");
            })
        }));
        assert!(result.is_err());

        // whatever the failed fill left behind is not part of the next one
        assert_eq!(fetched.observe::<u32>(|buffer| buffer.push(2)), [2]);
    }

    #[test]
    fn test_fetched_heap_size_is_kept_by_reset() {
        let mut fetched = Fetched::new();
        assert_eq!(fetched.heap_size(), 0);

        fetched.observe::<u32>(|buffer| buffer.extend(0..1000));
        assert!(fetched.heap_size() >= size_of::<u32>() * 1000);

        // the buffer keeps its capacity over a reset
        fetched.reset();
        assert!(fetched.heap_size() >= size_of::<u32>() * 1000);

        // and can be used again
        assert_eq!(fetched.observe::<u32>(|buffer| buffer.push(7)), [7]);
    }
}
