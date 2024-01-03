use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
};

use crate::backend::Event;

type Cmp<const SIZE: usize> =
    Box<dyn Fn(&Event<SIZE>, &Event<SIZE>) -> bool + Send + Sync + 'static>;

type Filter<const SIZE: usize> = Box<dyn Fn(&Event<SIZE>) -> bool + Send + Sync + 'static>;

pub enum Slot<const SIZE: usize> {
    All(Mutex<VecDeque<Event<SIZE>>>),
    Last(Mutex<VecDeque<Event<SIZE>>>),
    First(Mutex<VecDeque<Event<SIZE>>>),
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
            SlotType::First => Self::First(Mutex::new(VecDeque::with_capacity(1))),
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
    pub fn push(&self, value: Event<SIZE>) {
        match self {
            // store all events
            Self::All(lock) => {
                // we have full controll over the lock, there should never be a panick while holding the guard
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
                guard.push_back(value);
            }

            // store only the last
            Self::Last(lock) => {
                // we have full controll over the lock, there should never be a panick while holding the guard
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);

                // try to pop the current value
                _ = guard.pop_back();

                // insert new value
                guard.push_back(value);
            }

            // store only the first
            Self::First(lock) => {
                // we have full controll over the lock, there should never be a panick while holding the guard
                let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);

                // if no event is stored, store input
                if guard.is_empty() {
                    guard.push_front(value);
                }
            }

            // use custom compare function
            Self::Cmp { inner, cmp } => {
                // we have full controll over the lock, there should never be a panick while holding the guard
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

                // we have full controll over the lock, there should never be a panick while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);
                guard.push_back(value);
            }

            // store all events up to specified number
            Self::Max { inner, max } => {
                // we have full controll over the lock, there should never be a panick while holding the guard
                let mut guard = inner.lock().unwrap_or_else(PoisonError::into_inner);

                if guard.len() == *max {
                    // remove oldest value
                    guard.pop_front();
                }
                // put new value in
                guard.push_back(value);
            }
        }
    }

    #[inline]
    pub fn events(&self) -> MutexGuard<VecDeque<Event<SIZE>>> {
        let lock = match self {
            Self::All(lock) | Self::Last(lock) | Self::First(lock) => lock,
            Self::Cmp { inner, cmp: _ }
            | Self::AllFilter { inner, filter: _ }
            | Self::Max { inner, max: _ } => inner,
        };

        // we have full controll over the lock, there should never be a panick while holding the guard
        lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    pub fn events_clone(&self) -> VecDeque<Event<SIZE>> {
        let lock = match self {
            Self::All(lock) | Self::Last(lock) | Self::First(lock) => lock,
            Self::Cmp { inner, cmp: _ }
            | Self::AllFilter { inner, filter: _ }
            | Self::Max { inner, max: _ } => inner,
        };

        // we have full controll over the lock, there should never be a panick while holding the guard
        let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);

        // allocate new buffer
        let new = VecDeque::with_capacity((guard.len() / 2).min(1));

        // swap underlying buffer
        std::mem::replace(&mut *guard, new)
    }

    /// Frees all allocated memory.
    #[inline]
    pub fn cleanup(&self) {
        let lock = match self {
            Self::All(lock) | Self::Last(lock) | Self::First(lock) => lock,
            Self::Cmp { inner, cmp: _ }
            | Self::AllFilter { inner, filter: _ }
            | Self::Max { inner, max: _ } => inner,
        };

        let mut guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        *guard = VecDeque::new();
    }
}

impl<const EVENT_SIZE: usize> std::fmt::Debug for Slot<EVENT_SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(_) => f.debug_tuple("All").finish(),
            Self::Last(_) => f.debug_tuple("Last").finish(),
            Self::First(_) => f.debug_tuple("First").finish(),
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

    use crate::{backend::Event, SlotType};

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
}
