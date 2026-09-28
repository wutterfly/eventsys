use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

/// The stored events of a [`Slot`].
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

enum Kind<T> {
    All,
    Last,
    First,
    Cmp(fn(&T, &T) -> bool),
    AllFilter(fn(&T) -> bool),
    Max(usize),
}

pub struct Slot<T> {
    store: Mutex<Store<T>>,

    /// Set, while events are stored. Lets other threads skip locking, if there is nothing to do.
    ///
    /// Only changed while holding the lock of `store`. Set to `true` when an event gets stored and reset to `false`
    /// whenever the events get accessed for consumption. That way `true` always means an event is stored (until the
    /// events are consumed) and `false` means there is nothing to consume. Consumers always have to remove all events,
    /// before releasing the lock.
    ///
    /// Reading `false` without holding the lock, can miss an event that is stored at the same moment. Such an event
    /// simply is part of the next batch.
    filled: AtomicBool,

    kind: Kind<T>,
}

impl<T> Slot<T> {
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(typ: SlotType<T>) -> Self {
        let (kind, capacity) = match typ {
            SlotType::All => (Kind::All, 64),
            SlotType::Last => (Kind::Last, 1),
            SlotType::First => (Kind::First, 1),
            SlotType::Cmp(cmp) => (Kind::Cmp(cmp), 1),
            SlotType::AllFilter(filter) => (Kind::AllFilter(filter), 32),
            SlotType::Max(max) => (Kind::Max(max), max / 2),
        };

        Self {
            store: Mutex::new(Store::new(VecDeque::with_capacity(capacity))),
            filled: AtomicBool::new(false),
            kind,
        }
    }

    #[inline]
    // `filled` must only change while holding the lock, so the guard has to stay alive until then
    #[allow(clippy::significant_drop_tightening)]
    pub fn push(&self, value: T) {
        // check if the event can be discarded, without locking
        match &self.kind {
            // an event is already stored, nothing to do
            Kind::First if self.filled.load(Ordering::Relaxed) => return,

            // use custom filter function
            Kind::AllFilter(filter) if !filter(&value) => return,

            // nothing can be stored
            Kind::Max(0) => return,

            _ => {}
        }

        let mut guard = self.lock();

        match &self.kind {
            // store all events
            Kind::All | Kind::AllFilter(_) => guard.push_back(value),

            // store only the last
            Kind::Last => {
                // try to pop the current value
                _ = guard.pop_back();

                // insert new value
                guard.push_back(value);
            }

            // store only the first
            Kind::First => {
                // if no event is stored, store input
                if guard.is_empty() {
                    guard.push_front(value);
                }
            }

            // use custom compare function
            Kind::Cmp(cmp) => {
                if let Some(curr) = guard.front_mut() {
                    // check if value should be replaced
                    if cmp(curr, &value) {
                        *curr = value;
                    }
                } else {
                    guard.push_front(value);
                }
            }

            // store all events up to specified number
            Kind::Max(max) => {
                if guard.len() >= *max {
                    // remove oldest value
                    guard.pop_front();
                }

                // put new value in
                guard.push_back(value);
            }
        }

        self.filled.store(true, Ordering::Relaxed);
    }

    #[inline]
    fn lock(&self) -> MutexGuard<'_, Store<T>> {
        // we have full control over the lock, there should never be a panic while holding the guard
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks the stored events, to consume them.
    ///
    /// All events have to be removed, before the lock is released.
    #[inline]
    fn lock_for_consume(&self) -> MutexGuard<'_, Store<T>> {
        let guard = self.lock();

        // the events are about to be consumed, `push` and the other consumers have to check again under the lock
        self.filled.store(false, Ordering::Relaxed);

        guard
    }

    /// Locks the stored events, to consume them in place.
    ///
    /// All events have to be removed, before the lock is released.
    #[inline]
    pub fn events(&self) -> MutexGuard<'_, Store<T>> {
        self.lock_for_consume()
    }

    /// Takes all stored events out of the slot.
    ///
    /// Give the buffer back with [`Slot::recycle`], after all events are consumed.
    #[inline]
    pub fn events_clone(&self) -> VecDeque<T> {
        // nothing stored, no need to lock
        if !self.filled.load(Ordering::Relaxed) {
            return VecDeque::new();
        }

        let mut guard = self.lock_for_consume();

        // nothing to take, keep the current buffer and do not allocate a new one
        if guard.is_empty() {
            return VecDeque::new();
        }

        // new buffer for the next batch, that is expected to be about as big as this one
        let len = guard.len();
        let new = guard
            .spare
            .take()
            .unwrap_or_else(|| VecDeque::with_capacity(len));

        // swap underlying buffer
        std::mem::replace(&mut guard.events, new)
    }

    /// Hands back the buffer of a consumed batch, to be used for a later batch.
    #[inline]
    pub fn recycle(&self, buffer: VecDeque<T>) {
        debug_assert!(buffer.is_empty());

        if buffer.capacity() == 0 {
            return;
        }

        let mut guard = self.lock();
        if guard.spare.is_none() {
            guard.spare = Some(buffer);
        }
    }

    /// Frees all allocated memory.
    #[inline]
    pub fn cleanup(&self) {
        let mut guard = self.lock_for_consume();
        guard.events = VecDeque::new();
        guard.spare = None;
    }
}

impl<T> std::fmt::Debug for Slot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            Kind::All => f.debug_tuple("All").finish_non_exhaustive(),
            Kind::Last => f.debug_tuple("Last").finish_non_exhaustive(),
            Kind::First => f.debug_struct("First").finish_non_exhaustive(),
            Kind::Cmp(_) => f.debug_struct("Cmp").finish_non_exhaustive(),
            Kind::AllFilter(_) => f.debug_struct("AllFilter").finish_non_exhaustive(),
            Kind::Max(_) => f.debug_struct("Max").finish_non_exhaustive(),
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

    /// Only the first event of the matching type gets stored.
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

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
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

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e);
        }

        let first = values.pop().unwrap();
        assert_eq!(first, 0);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_last() {
        let slot = Slot::new(SlotType::Last);

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e);
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 99);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_cmp() {
        let slot = Slot::new(SlotType::Cmp(|current, next| *next > 2 * current));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e);
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 63);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_filter() {
        let slot = Slot::new(SlotType::AllFilter(|next| *next >= 50));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
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

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
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

        assert_eq!(slot.events().len(), 0);
    }

    #[test]
    fn test_slot_max_one() {
        let slot = Slot::new(SlotType::Max(1));

        for i in 0..100u32 {
            slot.push(i);
        }

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap(), 99);
    }

    #[test]
    fn test_slot_cmp_first_event_is_always_stored() {
        // compare function never accepts a replacement
        let slot = Slot::new(SlotType::Cmp(|_, _| false));

        for i in 5..100u32 {
            slot.push(i);
        }

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap(), 5);
    }

    #[test]
    fn test_slot_filter_rejects_all() {
        let slot = Slot::new(SlotType::AllFilter(|_| false));

        for i in 0..100u32 {
            slot.push(i);
        }

        assert_eq!(slot.events().len(), 0);
    }

    #[test]
    fn test_slot_events_clone_takes_events() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }

        let taken = slot.events_clone();
        let values = taken.into_iter().collect::<Vec<_>>();
        assert_eq!(values, (0..10).collect::<Vec<_>>());

        // slot is empty afterwards, but still usable
        assert_eq!(slot.events().len(), 0);

        slot.push(10u32);
        assert_eq!(slot.events().len(), 1);
    }

    #[test]
    fn test_slot_events_clone_empty() {
        let slot = Slot::new(SlotType::All);

        assert_eq!(slot.events_clone().len(), 0);

        slot.push(1u32);
        assert_eq!(slot.events().len(), 1);
    }

    #[test]
    fn test_slot_events_clone_keeps_capacity() {
        let slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }

        drop(slot.events_clone());

        // the replacement buffer is preallocated to half of the previous length
        assert!(slot.events().capacity() >= 50);
    }

    #[test]
    fn test_slot_events_clone_empty_keeps_buffer() {
        let slot = Slot::<u32>::new(SlotType::All);
        let capacity = slot.events().capacity();
        assert!(capacity > 0);

        // taking nothing neither allocates a new buffer, nor throws the current one away
        let taken = slot.events_clone();
        assert_eq!(taken.len(), 0);
        assert_eq!(taken.capacity(), 0);
        assert_eq!(slot.events().capacity(), capacity);
    }

    #[test]
    fn test_slot_first_rejects_while_filled() {
        let slot = Slot::new(SlotType::First);

        slot.push(1u32);
        slot.push(2u32);

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap(), 1);
    }

    #[test]
    fn test_slot_first_accepts_after_consume() {
        let slot = Slot::new(SlotType::First);

        slot.push(1u32);

        // consumed with events()
        _ = slot.events().pop_front();
        slot.push(2u32);
        assert_eq!(slot.events().pop_front().unwrap(), 2);

        // consumed with events_clone()
        slot.push(3u32);
        drop(slot.events_clone());
        slot.push(4u32);
        assert_eq!(slot.events().pop_front().unwrap(), 4);

        // consumed with cleanup()
        slot.push(5u32);
        slot.cleanup();
        slot.push(6u32);
        assert_eq!(slot.events().pop_front().unwrap(), 6);
    }

    #[test]
    fn test_slot_first_stays_filled_if_not_consumed() {
        let slot = Slot::new(SlotType::First);

        slot.push(1u32);

        // accessing the events without taking them out, must not make room for a new event
        drop(slot.events());
        slot.push(2u32);

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap(), 1);
    }

    #[test]
    fn test_slot_recycle_reuses_buffer() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }

        // take the events, the slot continues with a new buffer
        let mut taken = slot.events_clone();
        let capacity = taken.capacity();
        assert_eq!(taken.len(), 10);
        assert_ne!(slot.events().capacity(), capacity);

        // give the consumed buffer back
        taken.clear();
        slot.recycle(taken);

        // the next batch is taken with the recycled buffer
        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.events_clone().len(), 10);
        assert_eq!(slot.events().capacity(), capacity);
    }

    #[test]
    fn test_slot_recycle_keeps_only_one_buffer() {
        let slot = Slot::new(SlotType::All);

        for i in 0..10u32 {
            slot.push(i);
        }
        let mut first = slot.events_clone();
        let first_capacity = first.capacity();

        for i in 0..10u32 {
            slot.push(i);
        }
        let mut second = slot.events_clone();
        let second_capacity = second.capacity();
        assert_ne!(first_capacity, second_capacity);

        first.clear();
        second.clear();
        slot.recycle(first);
        // there already is a spare buffer, this one gets dropped
        slot.recycle(second);

        for i in 0..10u32 {
            slot.push(i);
        }
        drop(slot.events_clone());
        assert_eq!(slot.events().capacity(), first_capacity);
    }

    #[test]
    fn test_slot_recycle_ignores_empty_buffer() {
        let slot = Slot::new(SlotType::All);

        slot.recycle(std::collections::VecDeque::new());

        for i in 0..10u32 {
            slot.push(i);
        }
        assert_eq!(slot.events_clone().len(), 10);
    }

    #[test]
    fn test_slot_cleanup_drops_recycled_buffer() {
        let slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }
        let mut taken = slot.events_clone();
        let capacity = taken.capacity();
        taken.clear();
        slot.recycle(taken);

        slot.cleanup();

        // the recycled buffer is gone, a new (smaller) one gets allocated
        for i in 0..3u32 {
            slot.push(i);
        }
        drop(slot.events_clone());
        assert!(slot.events().capacity() < capacity);
    }

    #[test]
    fn test_slot_empty_poll_then_push() {
        let slot = Slot::new(SlotType::All);

        // polling without events does not hide the next event
        for _ in 0..3 {
            assert_eq!(slot.events_clone().len(), 0);
        }

        slot.push(1u32);

        let mut taken = slot.events_clone();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken.pop_front().unwrap(), 1);
        assert_eq!(slot.events_clone().len(), 0);
    }

    #[test]
    fn test_slot_filtered_events_do_not_mark_slot_filled() {
        let slot = Slot::new(SlotType::AllFilter(|_| false));

        for i in 0..10u32 {
            slot.push(i);
        }

        // nothing was stored, the buffer stays where it is
        let capacity = slot.events().capacity();
        assert_eq!(slot.events_clone().len(), 0);
        assert_eq!(slot.events().capacity(), capacity);
    }

    #[test]
    fn test_slot_cleanup() {
        let slot = Slot::new(SlotType::All);

        for i in 0..100u32 {
            slot.push(i);
        }

        slot.cleanup();
        assert_eq!(slot.events().len(), 0);
        assert_eq!(slot.events().capacity(), 0);

        slot.push(1u32);
        assert_eq!(slot.events().len(), 1);
    }
}
