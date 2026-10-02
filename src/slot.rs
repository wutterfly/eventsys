use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use anythingy::{AtomicSlot, EventQueue, HeapSize};

use crate::consumed::Consumed;

/// The stored events of a [`Slot::Store`] or [`Slot::Queue`] slot.
///
/// Dereferences to the stored events.
#[derive(Debug)]
pub struct Store<T> {
    events: VecDeque<T>,

    /// Buffer of an already consumed batch, that gets used for the next batch.
    /// This way batches do not need a new allocation each time.
    spare: Option<VecDeque<T>>,
}

impl<T> Store<T> {
    #[inline]
    const fn new(events: VecDeque<T>) -> Self {
        Self {
            events,
            spare: None,
        }
    }

    /// Takes whichever recycled buffer is available (`spare` preferred, then whatever `events` currently holds),
    /// leaving an empty one behind.
    ///
    /// Only used by [`Slot::Queue`], which has nothing of its own to accumulate into between pushes — events live
    /// in the lock-free [`EventQueue`] instead, so `events` and `spare` here are just recycled capacity waiting
    /// to be reused as the destination for the next batch.
    #[inline]
    fn take_scratch(&mut self) -> VecDeque<T> {
        self.spare
            .take()
            .unwrap_or_else(|| std::mem::take(&mut self.events))
    }

    /// Counts the ring buffer, plus the spare buffer kept for the next batch, if there is one.
    #[inline]
    fn heap_size(&self) -> usize {
        self.events.heap_size() + self.spare.as_ref().map_or(0, VecDeque::heap_size)
    }
}

impl<T> Deref for Store<T> {
    type Target = VecDeque<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl<T> DerefMut for Store<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}

/// What a [`Slot::Single`] slot does with a pushed event.
pub enum SingleKind<T> {
    /// Keep the first event until consumed, reject everything else.
    First,
    /// A user function decides whether the new event replaces the currently stored one.
    Cmp(fn(&T, &T) -> bool),
}

/// The event queue behind one registered type.
///
/// `First` and `Cmp` never hold more than one event, so they share a `Mutex<Option<T>>` with no `VecDeque` at
/// all — nothing to allocate, grow, or recycle. `Max` is the only `Store`-like kind that can hold more than one
/// event at once, so it keeps the `Mutex<VecDeque<T>>` with a recycled spare buffer. `Last` also never holds more
/// than one event, but needs no lock at all — see [`AtomicSlot`], a general-purpose primitive that (unlike
/// `Single`/`Store`/`Queue`'s `Mutex`/`EventQueue`) doesn't track its own emptiness, so `filled` lives here
/// instead, same as for every other kind.
///
/// `All` and `AllFilter` push into a lock-free `anythingy::EventQueue<T>` instead: pushing never waits on other
/// producer threads at all (no shared lock, unlike `Single`/`Store`), which fits them better since — unlike
/// `First`, `Cmp` and `Max` — nothing about them ever needs to reject or compare against what is already stored.
/// Consuming drains the queue into the `Mutex<Store<T>>`'s recycled buffer; since 0.3.2, `EventQueue` itself never
/// throws away a producer thread's buffer capacity on drain, so this stays allocation-free after warm-up, same
/// as `Store`. The trade-off: cross-thread push order is no longer guaranteed (only per-thread order is).
pub enum Slot<T> {
    /// `First`, `Cmp`.
    Single {
        cell: Mutex<Option<T>>,

        /// Set, while an event is stored. Lets other threads skip locking, if there is nothing to do.
        ///
        /// Only changed while holding the lock of `cell`. Set to `true` when an event gets stored and reset to
        /// `false` whenever the event gets accessed for consumption. That way `true` always means an event is
        /// stored (until it is consumed) and `false` means there is nothing to consume.
        ///
        /// Reading `false` without holding the lock, can miss an event that is stored at the same moment. Such
        /// an event simply is part of the next batch.
        filled: AtomicBool,

        kind: SingleKind<T>,
    },

    /// `Max`.
    Store {
        store: Mutex<Store<T>>,

        /// Same idea as [`Slot::Single`]'s `filled`.
        filled: AtomicBool,

        max: usize,
    },

    /// `All`, `AllFilter` (`filter` is `None` for `All`).
    Queue {
        queue: EventQueue<T>,

        /// Same idea as [`Slot::Single`]'s `filled`, but set right after a lock-free push, without holding any
        /// lock at all.
        filled: AtomicBool,

        /// Never touched by `push`; only used when events are consumed, to stage the drained batch.
        store: Mutex<Store<T>>,

        filter: Option<fn(&T) -> bool>,
    },

    /// `Last` — fully lock-free, see [`AtomicSlot`].
    Last {
        cell: AtomicSlot<T>,

        /// Same idea as [`Slot::Single`]'s `filled`, but set right after a lock-free push, without holding any
        /// lock at all. `AtomicSlot` itself has no such flag (a fast "is there anything to take" hint is not
        /// every caller's trade-off to make), so `Slot` keeps it, same as for `Store`/`Queue`.
        filled: AtomicBool,
    },
}

impl<T: Send> Slot<T> {
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(typ: SlotType<T>) -> Self {
        match typ {
            SlotType::All => Self::new_queue(64, None),
            SlotType::AllFilter(filter) => Self::new_queue(32, Some(filter)),
            SlotType::First => Self::new_single(SingleKind::First),
            SlotType::Cmp(cmp) => Self::new_single(SingleKind::Cmp(cmp)),
            SlotType::Max(max) => Self::new_store(max),
            SlotType::Last => Self::new_last(),
        }
    }

    #[inline]
    const fn new_single(kind: SingleKind<T>) -> Self {
        Self::Single {
            cell: Mutex::new(None),
            filled: AtomicBool::new(false),
            kind,
        }
    }

    #[inline]
    fn new_store(max: usize) -> Self {
        Self::Store {
            store: Mutex::new(Store::new(VecDeque::with_capacity(max / 2))),
            filled: AtomicBool::new(false),
            max,
        }
    }

    #[inline]
    fn new_queue(capacity: usize, filter: Option<fn(&T) -> bool>) -> Self {
        Self::Queue {
            queue: EventQueue::new(),
            filled: AtomicBool::new(false),
            store: Mutex::new(Store::new(VecDeque::with_capacity(capacity))),
            filter,
        }
    }

    #[inline]
    fn new_last() -> Self {
        Self::Last {
            cell: AtomicSlot::new(),
            filled: AtomicBool::new(false),
        }
    }

    #[inline]
    // `filled` must only change while holding the lock, so the guard has to stay alive until then
    #[allow(clippy::significant_drop_tightening)]
    pub fn push(&self, value: T) {
        match self {
            Self::Single { cell, filled, kind } => {
                // an event is already stored and nothing but a new push can change that, so skip locking
                if matches!(kind, SingleKind::First) && filled.load(Ordering::Relaxed) {
                    return;
                }

                let mut guard = cell.lock().unwrap_or_else(PoisonError::into_inner);

                match kind {
                    // store only the first
                    SingleKind::First => {
                        if guard.is_none() {
                            *guard = Some(value);
                        }
                    }

                    // use custom compare function
                    SingleKind::Cmp(cmp) => {
                        if let Some(curr) = guard.as_mut() {
                            // check if value should be replaced
                            if cmp(curr, &value) {
                                *curr = value;
                            }
                        } else {
                            *guard = Some(value);
                        }
                    }
                }

                filled.store(true, Ordering::Relaxed);
            }

            Self::Store { store, filled, max } => {
                // nothing can be stored
                if *max == 0 {
                    return;
                }

                let mut guard = store.lock().unwrap_or_else(PoisonError::into_inner);

                // store all events up to specified number
                if guard.len() >= *max {
                    // remove oldest value
                    guard.pop_front();
                }

                // put new value in
                guard.push_back(value);

                filled.store(true, Ordering::Relaxed);
            }

            // lock-free: no other producer is ever waited on
            Self::Queue {
                queue,
                filled,
                filter,
                ..
            } => {
                if filter.is_some_and(|f| !f(&value)) {
                    return;
                }

                queue.push(value);
                filled.store(true, Ordering::Relaxed);
            }

            // lock-free: a single atomic swap, no lock and no read of the previous value
            Self::Last { cell, filled } => {
                cell.push(value);
                filled.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Takes all currently available events out of the slot, without blocking producers.
    ///
    /// Only the check, whether there is anything to take, is done here. Most slots are empty most of the time, so
    /// this is by far the most common case, and keeping it small lets it be inlined into the caller and folded
    /// away. Actually taking the events is [`Slot::take_stored`], which is kept out of line for that reason.
    #[inline]
    pub fn consume(&self) -> Consumed<'_, T> {
        let (Self::Single { filled, .. }
        | Self::Store { filled, .. }
        | Self::Queue { filled, .. }
        | Self::Last { filled, .. }) = self;

        // nothing stored, no need to lock
        if !filled.load(Ordering::Relaxed) {
            return Consumed::taken(None);
        }

        self.take_stored()
    }

    /// Takes the events out of a slot that has some (as far as [`Slot::consume`] could tell).
    #[inline(never)]
    fn take_stored(&self) -> Consumed<'_, T> {
        match self {
            Self::Single { cell, filled, .. } => {
                let mut guard = cell.lock().unwrap_or_else(PoisonError::into_inner);
                filled.store(false, Ordering::Relaxed);

                Consumed::taken(guard.take())
            }

            Self::Store { store, filled, .. } => {
                let mut guard = store.lock().unwrap_or_else(PoisonError::into_inner);
                filled.store(false, Ordering::Relaxed);

                // nothing to take, keep the current buffer and do not allocate a new one
                if guard.is_empty() {
                    return Consumed::taken(None);
                }

                // new buffer for the next batch, that is expected to be about as big as this one
                let len = guard.len();
                let new = guard
                    .spare
                    .take()
                    .unwrap_or_else(|| VecDeque::with_capacity(len));

                // swap underlying buffer
                let mut taken = std::mem::replace(&mut guard.events, new);
                drop(guard);

                // this ring buffer is the only place events can end up wrapped around its end (events that were
                // replaced at the front), and `Consumed::as_slice` needs them in one piece
                taken.make_contiguous();

                Consumed::owned(taken, self)
            }

            Self::Queue {
                queue,
                filled,
                store,
                ..
            } => {
                let mut guard = store.lock().unwrap_or_else(PoisonError::into_inner);
                filled.store(false, Ordering::Relaxed);

                let mut buf = Vec::from(guard.take_scratch());
                queue.drain_into(&mut buf);
                drop(guard);

                Consumed::owned(VecDeque::from(buf), self)
            }

            Self::Last { cell, filled } => {
                filled.store(false, Ordering::Relaxed);
                Consumed::taken(cell.take())
            }
        }
    }

    /// Drops all stored events, but keeps everything that was allocated for them, so the slot can be filled again
    /// without allocating.
    #[inline]
    pub fn clear(&mut self) {
        match self {
            Self::Single { cell, filled, .. } => {
                filled.store(false, Ordering::Relaxed);
                *cell.get_mut().unwrap_or_else(PoisonError::into_inner) = None;
            }
            Self::Store { store, filled, .. } => {
                filled.store(false, Ordering::Relaxed);
                // the spare buffer stays as it is
                store
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner)
                    .events
                    .clear();
            }
            Self::Queue {
                queue,
                filled,
                store,
                ..
            } => {
                // drops the events, and hands the emptied buffers of every producer thread back to the queue
                queue.drain_each(|_| {});
                filled.store(false, Ordering::Relaxed);
                store
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner)
                    .events
                    .clear();
            }
            Self::Last { cell, filled } => {
                filled.store(false, Ordering::Relaxed);
                cell.clear();
            }
        }
    }
}

impl<T> Slot<T> {
    /// Hands back the buffer of a consumed `Store`/`Queue` batch, to be used for a later batch.
    ///
    /// A no-op for `Single`/`Last`, neither of which ever allocates a separate buffer to begin with. Kept in its
    /// own `impl<T>` block, without the `Send` bound the rest of `Slot`'s methods need (they touch
    /// `EventQueue<T>`, which requires it): this one only ever touches the plain `Mutex<Store<T>>`, and
    /// [`Consumed`]'s `Drop` calls it unconditionally, so requiring `T: Send` here would force that bound
    /// onto `Consumed` itself.
    #[inline]
    pub fn recycle(&self, buffer: VecDeque<T>) {
        debug_assert!(buffer.is_empty());

        if buffer.capacity() == 0 {
            return;
        }

        let store = match self {
            Self::Store { store, .. } | Self::Queue { store, .. } => store,
            Self::Single { .. } | Self::Last { .. } => return,
        };

        let mut guard = store.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.spare.is_none() {
            guard.spare = Some(buffer);
        }
    }

    /// Counts the heap memory this slot has allocated beyond its own inline bytes, as far as each variant's own
    /// storage can tell — see [`anythingy::HeapSize`] for how to read the result.
    ///
    /// `Single` never allocates: its `Mutex<Option<T>>` stores the value inline, so it reports `0`.
    #[inline]
    pub fn heap_size(&self) -> usize {
        match self {
            Self::Single { .. } => 0,
            Self::Store { store, .. } => store
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .heap_size(),
            Self::Queue { queue, store, .. } => {
                queue.heap_size()
                    + store
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .heap_size()
            }
            Self::Last { cell, .. } => cell.heap_size(),
        }
    }
}

impl<T> std::fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queue { filter: None, .. } => f.debug_tuple("All").finish_non_exhaustive(),
            Self::Queue {
                filter: Some(_), ..
            } => f.debug_struct("AllFilter").finish_non_exhaustive(),
            Self::Single {
                kind: SingleKind::First,
                ..
            } => f.debug_struct("First").finish_non_exhaustive(),
            Self::Single {
                kind: SingleKind::Cmp(_),
                ..
            } => f.debug_struct("Cmp").finish_non_exhaustive(),
            Self::Store { .. } => f.debug_struct("Max").finish_non_exhaustive(),
            Self::Last { .. } => f.debug_struct("Last").finish_non_exhaustive(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Specifies what events get stored.
pub enum SlotType<T> {
    /// All events of the matching type get stored.
    All,

    /// Only the last event of the matching type gets stored.
    Last,

    /// Only the first event of the matching type gets stored, until consumed.
    First,

    /// A user specified function gets called to decide if the new event replaces the currently stored event.
    /// Return `true`, if the current event should get replaced. Else return `false`.
    Cmp(fn(current: &T, new: &T) -> bool),

    /// A user specified function gets called to decide if the new event should be kept.
    /// Return `true` if the event should be kept, else return `false` if the event should be discarded.
    AllFilter(fn(new: &T) -> bool),

    /// Collect all events until number is reached.
    ///
    /// Any more events replace the oldest events.
    Max(usize),
}

#[cfg(test)]
mod tests {

    use crate::SlotType;

    use super::Slot;

    #[test]
    fn test_slot_all() {
        let slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(100);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        assert_eq!(values, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn test_slot_first() {
        let slot = Slot::new(SlotType::First);

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        let first = values.pop().unwrap();
        assert_eq!(first, 0);
        assert_eq!(values.len(), 0);
    }

    #[test]
    fn test_slot_last() {
        let slot = Slot::new(SlotType::Last);

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 99);
        assert_eq!(values.len(), 0);
    }

    #[test]
    fn test_slot_cmp() {
        let slot = Slot::new(SlotType::Cmp(|current, next| *next > 2 * current));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 63);
        assert_eq!(values.len(), 0);
    }

    #[test]
    fn test_slot_filter() {
        let slot = Slot::new(SlotType::AllFilter(|next| *next >= 50));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        assert_eq!(values, (50..100).collect::<Vec<_>>());
        assert_eq!(values.len(), 50);
    }

    #[test]
    fn test_slot_max() {
        let slot = Slot::new(SlotType::Max(100));

        for i in 0..200u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(100);

        let consumed = slot.consume();
        for e in consumed {
            values.push(e);
        }

        assert_eq!(values, (100..200).collect::<Vec<_>>());
        assert_eq!(values.len(), 100);
    }

    #[test]
    fn test_slot_max_zero() {
        let slot = Slot::new(SlotType::Max(0));

        for i in 0..100u32 {
            slot.push(i);
        }

        assert_eq!(slot.consume().len(), 0);
    }

    #[test]
    fn test_slot_max_one() {
        let slot = Slot::new(SlotType::Max(1));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed.next().unwrap(), 99);
    }

    #[test]
    fn test_slot_cmp_first_event_is_always_stored() {
        // compare function never accepts a replacement
        let slot = Slot::new(SlotType::Cmp(|_, _| false));

        for i in 5..100u32 {
            slot.push(i);
        }

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed.next().unwrap(), 5);
    }

    #[test]
    fn test_slot_filter_rejects_all() {
        let slot = Slot::new(SlotType::AllFilter(|_| false));

        for i in 0..100u32 {
            slot.push(i);
        }

        assert_eq!(slot.consume().len(), 0);
    }

    #[test]
    fn test_slot_consume_takes_events() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }

        let taken = slot.consume();
        let values = taken.collect::<Vec<_>>();
        assert_eq!(values, (0..10).collect::<Vec<_>>());

        // slot is empty afterwards, but still usable
        assert_eq!(slot.consume().len(), 0);

        slot.push(10u32);
        assert_eq!(slot.consume().len(), 1);
    }

    #[test]
    fn test_slot_consume_empty() {
        let slot = Slot::new(SlotType::All);

        assert_eq!(slot.consume().len(), 0);

        slot.push(1u32);
        assert_eq!(slot.consume().len(), 1);
    }

    #[test]
    fn test_slot_consume_keeps_capacity() {
        let slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }
        drop(slot.consume());

        // the buffer swapped in by the previous take is preallocated to about the previous length
        for i in 0..50u32 {
            slot.push(i);
        }
        assert!(slot.consume().capacity() >= 50);
    }

    #[test]
    fn test_slot_consume_empty_keeps_buffer() {
        let slot = Slot::<u32>::new(SlotType::All);

        // taking nothing neither allocates a new buffer, nor throws the current one away
        let taken = slot.consume();
        assert_eq!(taken.len(), 0);
        assert_eq!(taken.capacity(), 0);
    }

    #[test]
    fn test_slot_first_rejects_while_filled() {
        let slot = Slot::new(SlotType::First);

        slot.push(1u32);
        slot.push(2u32);

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed.next().unwrap(), 1);
    }

    #[test]
    fn test_slot_first_accepts_after_consume() {
        let mut slot = Slot::new(SlotType::First);

        // consumed with consume()
        slot.push(1u32);
        drop(slot.consume());
        slot.push(2u32);
        assert_eq!(slot.consume().next().unwrap(), 2);

        // consumed with clear()
        slot.push(3u32);
        slot.clear();
        slot.push(4u32);
        assert_eq!(slot.consume().next().unwrap(), 4);
    }

    #[test]
    fn test_slot_first_consume_frees_the_slot_even_if_unconsumed() {
        let slot = Slot::new(SlotType::First);

        slot.push(1u32);

        // every consume is all-or-nothing: merely acquiring and dropping one discards event 1, even though it was
        // never actually read, freeing the slot for a new push
        drop(slot.consume());
        slot.push(2u32);

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed.next().unwrap(), 2);
    }

    #[test]
    fn test_slot_last_accepts_after_consume() {
        let mut slot = Slot::new(SlotType::Last);

        // consumed with consume()
        slot.push(1u32);
        drop(slot.consume());
        slot.push(2u32);
        assert_eq!(slot.consume().next().unwrap(), 2);

        // consumed with clear()
        slot.push(3u32);
        slot.clear();
        slot.push(4u32);
        assert_eq!(slot.consume().next().unwrap(), 4);
    }

    #[test]
    fn test_slot_last_overwrites_while_unconsumed() {
        let slot = Slot::new(SlotType::Last);

        slot.push(1u32);
        // not consumed yet, but Last always overwrites (unlike First, which would reject this)
        slot.push(2u32);

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed.next().unwrap(), 2);
    }

    #[test]
    fn test_slot_recycle_reuses_buffer() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }

        // take the events, give the buffer back for reuse
        let taken = slot.consume();
        let capacity = taken.capacity();
        assert_eq!(taken.len(), 10);
        drop(taken);

        // the next batch swaps in the recycled buffer as its own replacement, so it resurfaces one round later
        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.consume().len(), 10);

        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.consume().capacity(), capacity);
    }

    #[test]
    fn test_slot_recycle_keeps_only_one_buffer() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }
        let first = slot.consume();
        let first_capacity = first.capacity();

        for i in 0..10u32 {
            slot.push(i);
        }
        let second = slot.consume();
        let second_capacity = second.capacity();
        assert_ne!(first_capacity, second_capacity);

        drop(first);
        // there already is a spare buffer, this one gets dropped
        drop(second);

        // only `first`'s buffer survived as the spare; it resurfaces two batches later
        for i in 0..10u32 {
            slot.push(i);
        }
        drop(slot.consume());

        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.consume().capacity(), first_capacity);
    }

    #[test]
    fn test_slot_recycle_ignores_empty_buffer() {
        let slot = Slot::new(SlotType::All);

        slot.recycle(std::collections::VecDeque::new());

        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.consume().len(), 10);
    }

    #[test]
    fn test_slot_clear_keeps_recycled_buffer() {
        let mut slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }
        let taken = slot.consume();
        let capacity = taken.capacity();
        drop(taken);

        slot.clear();

        // the recycled buffer is still there, and gets used for the next batch
        for i in 0..3u32 {
            slot.push(i);
        }
        assert!(slot.consume().capacity() >= capacity);
    }

    #[test]
    fn test_slot_clear_keeps_the_buffers_of_the_queue() {
        let mut slot = Slot::new(SlotType::All);

        for i in 0..1000u32 {
            slot.push(i);
        }
        let before = slot.heap_size();
        assert!(before >= size_of::<u32>() * 1000);

        // dropping the events keeps what was allocated for them
        slot.clear();
        assert_eq!(slot.consume().len(), 0);
        assert!(slot.heap_size() >= before);
    }

    #[test]
    fn test_slot_clear_keeps_the_buffer_of_max() {
        let mut slot = Slot::new(SlotType::Max(1000));

        for i in 0..1000u32 {
            slot.push(i);
        }
        let before = slot.heap_size();
        assert!(before >= size_of::<u32>() * 1000);

        slot.clear();
        assert_eq!(slot.consume().len(), 0);
        assert!(slot.heap_size() >= before);
    }

    #[test]
    fn test_slot_clear_drops_the_events() {
        use std::sync::Arc;

        let probe = Arc::new(());

        for typ in [
            SlotType::All,
            SlotType::AllFilter(|_| true),
            SlotType::Max(10),
            SlotType::First,
            SlotType::Last,
            SlotType::Cmp(|_, _| true),
        ] {
            let mut slot = Slot::new(typ);

            for _ in 0..3 {
                slot.push(probe.clone());
            }
            assert!(Arc::strong_count(&probe) > 1);

            slot.clear();
            assert_eq!(Arc::strong_count(&probe), 1, "{slot:?}");
        }
    }

    #[test]
    fn test_slot_empty_poll_then_push() {
        let slot = Slot::new(SlotType::All);

        // polling without events does not hide the next event
        for _ in 0..3 {
            assert_eq!(slot.consume().len(), 0);
        }

        slot.push(1u32);

        let mut taken = slot.consume();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken.next().unwrap(), 1);
        assert_eq!(slot.consume().len(), 0);
    }

    #[test]
    fn test_slot_filtered_events_do_not_mark_slot_filled() {
        let slot = Slot::new(SlotType::AllFilter(|_| false));

        for i in 0..10u32 {
            slot.push(i);
        }

        // nothing was stored, so there is nothing to take
        assert_eq!(slot.consume().len(), 0);
    }

    #[test]
    fn test_slot_clear() {
        let mut slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }

        slot.clear();
        assert_eq!(slot.consume().len(), 0);

        // the slot is still usable
        slot.push(1u32);
        assert_eq!(slot.consume().len(), 1);
    }

    #[test]
    fn test_slot_queue_threads_lose_nothing() {
        // multiple producer threads pushing concurrently into an `All` slot: order between threads is not
        // guaranteed, but no event may be lost or duplicated.
        const THREADS: u32 = 8;
        const PER_THREAD: u32 = 2000;

        let slot = Slot::new(SlotType::All);

        std::thread::scope(|s| {
            for t in 0..THREADS {
                let slot = &slot;
                s.spawn(move || {
                    for i in 0..PER_THREAD {
                        slot.push(t * PER_THREAD + i);
                    }
                });
            }
        });

        let mut values = slot.consume().collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, (0..THREADS * PER_THREAD).collect::<Vec<_>>());
    }
}
