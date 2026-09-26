use std::{
    collections::VecDeque,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::backend::Event;

type Cmp<const SIZE: usize> =
    Box<dyn Fn(&Event<SIZE>, &Event<SIZE>) -> bool + Send + Sync + 'static>;

type Filter<const SIZE: usize> = Box<dyn Fn(&Event<SIZE>) -> bool + Send + Sync + 'static>;

pub enum Slot<const SIZE: usize> {
    All(Mutex<VecDeque<Event<SIZE>>>),
    Last(Mutex<VecDeque<Event<SIZE>>>),
    First {
        inner: Mutex<VecDeque<Event<SIZE>>>,

        /// Set, while an event is stored. Lets [`Slot::push`] skip locking, if there is nothing to do.
        ///
        /// Only changed while holding the lock of `inner`. Set to `true` when an event gets stored and reset to
        /// `false` whenever the events get accessed for consumption. That way `true` always means an event is stored.
        /// `false` is only a hint and gets checked again under the lock.
        filled: AtomicBool,
    },
    Cmp {
        inner: Mutex<VecDeque<Event<SIZE>>>,
        cmp: Cmp<SIZE>,
    },

    AllFilter {
        inner: Mutex<VecDeque<Event<SIZE>>>,
        filter: Filter<SIZE>,
    },
    Max {
        inner: Mutex<VecDeque<Event<SIZE>>>,
        max: usize,
    },
}

impl<const SIZE: usize> Slot<SIZE> {
    #[inline]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new<T: 'static>(typ: SlotType<T>) -> Self {
        match typ {
            SlotType::All => Self::All(Mutex::new(VecDeque::with_capacity(64))),
            SlotType::Last => Self::Last(Mutex::new(VecDeque::with_capacity(1))),
            SlotType::First => Self::First {
                inner: Mutex::new(VecDeque::with_capacity(1)),
                filled: AtomicBool::new(false),
            },
            SlotType::Cmp(cmp) => {
                let f = move |current: &Event<SIZE>, new: &Event<SIZE>| {
                    let c = current.get_ref::<T>();
                    let n = new.get_ref::<T>();

                    cmp(c, n)
                };

                Self::Cmp {
                    inner: Mutex::new(VecDeque::with_capacity(1)),
                    cmp: Box::new(f),
                }
            }
            SlotType::AllFilter(filter) => {
                let f = move |new: &Event<SIZE>| {
                    let n = new.get_ref::<T>();

                    filter(n)
                };

                Self::AllFilter {
                    inner: Mutex::new(VecDeque::with_capacity(32)),
                    filter: Box::new(f),
                }
            }
            SlotType::Max(max) => Self::Max {
                inner: Mutex::new(VecDeque::with_capacity(max / 2)),
                max,
            },
        }
    }

    #[inline]
    // `First` must only change `filled` while holding the lock, so its guard has to stay alive until then
    #[allow(clippy::significant_drop_tightening)]
    pub fn push(&self, value: Event<SIZE>) {
        match self {
            // store all events
            Self::All(lock) => {
                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
                guard.push_back(value);
            }

            // store only the last
            Self::Last(lock) => {
                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);

                // try to pop the current value
                _ = guard.pop_back();

                // insert new value
                guard.push_back(value);
            }

            // store only the first
            Self::First { inner, filled } => {
                // an event is already stored, nothing to do
                if filled.load(Ordering::Relaxed) {
                    return;
                }

                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);

                // if no event is stored, store input
                if guard.is_empty() {
                    guard.push_front(value);
                    filled.store(true, Ordering::Relaxed);
                }
            }

            // use custom compare function
            Self::Cmp { inner, cmp } => {
                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);

                if let Some(curr) = guard.front_mut() {
                    // check if value should be replaced
                    if cmp(curr, &value) {
                        *curr = value;
                    }
                } else {
                    guard.push_front(value);
                }
            }

            // use custom filter function
            Self::AllFilter { inner, filter: cmp } => {
                if !cmp(&value) {
                    return;
                }

                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);
                guard.push_back(value);
            }

            // store all events up to specified number
            Self::Max { inner, max } => {
                // nothing can be stored
                if *max == 0 {
                    return;
                }

                // we have full control over the lock, there should never be a panic while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);

                if guard.len() >= *max {
                    // remove oldest value
                    guard.pop_front();
                }
                // put new value in
                guard.push_back(value);
            }
        }
    }

    #[inline]
    const fn inner(&self) -> &Mutex<VecDeque<Event<SIZE>>> {
        match self {
            Self::All(inner)
            | Self::Last(inner)
            | Self::First { inner, filled: _ }
            | Self::Cmp { inner, cmp: _ }
            | Self::AllFilter { inner, filter: _ }
            | Self::Max { inner, max: _ } => inner,
        }
    }

    /// Locks the stored events, to consume them.
    #[inline]
    fn lock_for_consume(&self) -> MutexGuard<'_, VecDeque<Event<SIZE>>> {
        // we have full control over the lock, there should never be a panic while holding the guard
        let guard = self.inner().lock().unwrap_or_else(PoisonError::into_inner);

        // the events are about to be consumed, `push` has to check again under the lock
        if let Self::First { filled, inner: _ } = self {
            filled.store(false, Ordering::Relaxed);
        }

        guard
    }

    #[inline]
    pub fn events(&self) -> MutexGuard<'_, VecDeque<Event<SIZE>>> {
        self.lock_for_consume()
    }

    #[inline]
    pub fn events_clone(&self) -> VecDeque<Event<SIZE>> {
        let mut guard = self.lock_for_consume();

        // nothing to take, keep the current buffer and do not allocate a new one
        if guard.is_empty() {
            return VecDeque::new();
        }

        // allocate new buffer
        let new = VecDeque::with_capacity(guard.len() / 2);

        // swap underlying buffer
        std::mem::replace(&mut *guard, new)
    }

    /// Frees all allocated memory.
    #[inline]
    pub fn cleanup(&self) {
        let mut guard = self.lock_for_consume();
        *guard = VecDeque::new();
    }
}

impl<const EVENT_SIZE: usize> std::fmt::Debug for Slot<EVENT_SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(_) => f.debug_tuple("All").finish(),
            Self::Last(_) => f.debug_tuple("Last").finish(),
            Self::First { .. } => f.debug_struct("First").finish(),
            Self::Cmp { .. } => f.debug_struct("Cmp").finish(),
            Self::AllFilter { .. } => f.debug_struct("AllFilter").finish(),
            Self::Max { .. } => f.debug_struct("Max").finish(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Specifies what events get stored.
pub enum SlotType<T: 'static> {
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

    use crate::{SlotType, backend::Event};

    use super::Slot;

    #[test]
    fn test_slot_all() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(100);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        assert_eq!(values, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn test_slot_first() {
        let slot = Slot::<16>::new::<u32>(SlotType::First);

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        let first = values.pop().unwrap();
        assert_eq!(first, 0);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_last() {
        let slot = Slot::<16>::new::<u32>(SlotType::Last);

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 99);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_cmp() {
        let slot = Slot::<16>::new::<u32>(SlotType::Cmp(|current, next| *next > 2 * current));

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        let last = values.pop().unwrap();
        assert_eq!(last, 63);
        assert!(values.is_empty());
    }

    #[test]
    fn test_slot_filter() {
        let slot = Slot::<16>::new::<u32>(SlotType::AllFilter(|next| *next >= 50));

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(1);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        assert_eq!(values, (50..100).collect::<Vec<_>>());
        assert_eq!(values.len(), 50);
    }

    #[test]
    fn test_slot_max() {
        let slot = Slot::<16>::new::<u32>(SlotType::Max(100));

        for i in 0..200u32 {
            slot.push(Event::new(i));
        }

        let mut values = Vec::with_capacity(100);

        let mut query = slot.events();
        while let Some(e) = query.pop_front() {
            values.push(e.get::<u32>());
        }

        assert_eq!(values, (100..200).collect::<Vec<_>>());
        assert_eq!(values.len(), 100);
    }

    #[test]
    fn test_slot_max_zero() {
        let slot = Slot::<16>::new::<u32>(SlotType::Max(0));

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        assert_eq!(slot.events().len(), 0);
    }

    #[test]
    fn test_slot_max_one() {
        let slot = Slot::<16>::new::<u32>(SlotType::Max(1));

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap().get::<u32>(), 99);
    }

    #[test]
    fn test_slot_cmp_first_event_is_always_stored() {
        // compare function never accepts a replacement
        let slot = Slot::<16>::new::<u32>(SlotType::Cmp(|_, _| false));

        for i in 5..100u32 {
            slot.push(Event::new(i));
        }

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap().get::<u32>(), 5);
    }

    #[test]
    fn test_slot_filter_rejects_all() {
        let slot = Slot::<16>::new::<u32>(SlotType::AllFilter(|_| false));

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        assert_eq!(slot.events().len(), 0);
    }

    #[test]
    fn test_slot_events_clone_takes_events() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);

        for i in 0..10u32 {
            slot.push(Event::new(i));
        }

        let taken = slot.events_clone();
        let values = taken.into_iter().map(Event::get::<u32>).collect::<Vec<_>>();
        assert_eq!(values, (0..10).collect::<Vec<_>>());

        // slot is empty afterwards, but still usable
        assert_eq!(slot.events().len(), 0);

        slot.push(Event::new(10u32));
        assert_eq!(slot.events().len(), 1);
    }

    #[test]
    fn test_slot_events_clone_empty() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);

        assert_eq!(slot.events_clone().len(), 0);

        slot.push(Event::new(1u32));
        assert_eq!(slot.events().len(), 1);
    }

    #[test]
    fn test_slot_events_clone_keeps_capacity() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        drop(slot.events_clone());

        // the replacement buffer is preallocated to half of the previous length
        assert!(slot.events().capacity() >= 50);
    }

    #[test]
    fn test_slot_events_clone_empty_keeps_buffer() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);
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
        let slot = Slot::<16>::new::<u32>(SlotType::First);

        slot.push(Event::new(1u32));
        slot.push(Event::new(2u32));

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap().get::<u32>(), 1);
    }

    #[test]
    fn test_slot_first_accepts_after_consume() {
        let slot = Slot::<16>::new::<u32>(SlotType::First);

        slot.push(Event::new(1u32));

        // consumed with events()
        drop(slot.events().pop_front());
        slot.push(Event::new(2u32));
        assert_eq!(slot.events().pop_front().unwrap().get::<u32>(), 2);

        // consumed with events_clone()
        slot.push(Event::new(3u32));
        drop(slot.events_clone());
        slot.push(Event::new(4u32));
        assert_eq!(slot.events().pop_front().unwrap().get::<u32>(), 4);

        // consumed with cleanup()
        slot.push(Event::new(5u32));
        slot.cleanup();
        slot.push(Event::new(6u32));
        assert_eq!(slot.events().pop_front().unwrap().get::<u32>(), 6);
    }

    #[test]
    fn test_slot_first_stays_filled_if_not_consumed() {
        let slot = Slot::<16>::new::<u32>(SlotType::First);

        slot.push(Event::new(1u32));

        // accessing the events without taking them out, must not make room for a new event
        drop(slot.events());
        slot.push(Event::new(2u32));

        let mut query = slot.events();
        assert_eq!(query.len(), 1);
        assert_eq!(query.pop_front().unwrap().get::<u32>(), 1);
    }

    #[test]
    fn test_slot_cleanup() {
        let slot = Slot::<16>::new::<u32>(SlotType::All);

        for i in 0..100u32 {
            slot.push(Event::new(i));
        }

        slot.cleanup();
        assert_eq!(slot.events().len(), 0);
        assert_eq!(slot.events().capacity(), 0);

        slot.push(Event::new(1u32));
        assert_eq!(slot.events().len(), 1);
    }
}
