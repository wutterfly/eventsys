use std::{collections::VecDeque, fmt};

use crate::slot::Slot;

/// An iterator over events of type `T`, which owns them, see
/// [`EventBackend::consume`](crate::EventBackend::consume).
///
/// The events were taken out of the event system up front (a snapshot at construction time).
///
/// The counterpart of [`Observed`](crate::Observed), which only borrows events that stay in the event system. Both are
/// used the same way: they iterate over the events (`T` for `Consumed`, `&T` for `Observed`), know how many are left,
/// and can show them as a slice.
///
/// Neither `Consumed` nor `Observed` is `Clone` or `Copy`.
///
/// ```compile_fail
/// fn assert_clone<T: Clone>() {}
/// assert_clone::<eventsys::Consumed<u32>>();
/// ```
pub enum Consumed<'a, T> {
    /// `All`, `AllFilter`, `Cmp`, `Max`, `First` — the whole batch, swapped out of the slot at construction. The
    /// buffer is handed back to the slot for reuse when this is dropped.
    Owned {
        events: VecDeque<T>,
        slot: &'a Slot<T>,
    },

    /// `Last` — the at-most-one pending event, already taken out of the slot at construction.
    Taken(Option<T>),
}

impl<'a, T> Consumed<'a, T> {
    /// `events` have to be in one piece (not wrapped around the end of the ring buffer), so `as_slice` can hand out
    /// all of them at once. Consuming events from the front keeps them in one piece.
    ///
    /// Events that came out of a `Vec` are, and so are no events at all. Only a ring buffer that was filled and
    /// emptied from both ends can be wrapped, which is why it is up to the caller to know, and not checked here: it
    /// is on the path of every consume, including the ones that find nothing.
    #[inline]
    pub(crate) fn owned(events: VecDeque<T>, slot: &'a Slot<T>) -> Self {
        debug_assert!(
            events.as_slices().1.is_empty(),
            "the events have to be contiguous"
        );

        Self::Owned { events, slot }
    }

    #[inline]
    pub(crate) const fn taken(value: Option<T>) -> Self {
        Self::Taken(value)
    }

    /// Returns the number of events this `Consumed` can produce.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Owned { events, .. } => events.len(),
            Self::Taken(value) => usize::from(value.is_some()),
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The events that were not consumed yet, as a slice, in the order they are produced in. Every event that is
    /// consumed is removed from the front of it.
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::Owned { events, .. } => {
                let (events, rest) = events.as_slices();
                debug_assert!(rest.is_empty(), "`owned` makes the events contiguous");
                events
            }
            Self::Taken(value) => value.as_slice(),
        }
    }
}

impl<T> Iterator for Consumed<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Owned { events, .. } => events.pop_front(),
            Self::Taken(value) => value.take(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for Consumed<'_, T> {}

impl<T: fmt::Debug> fmt::Debug for Consumed<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Consumed").field(&self.as_slice()).finish()
    }
}

impl<T> Drop for Consumed<'_, T> {
    #[inline]
    fn drop(&mut self) {
        // drop all events that were not consumed, then hand the buffer back for reuse
        if let Self::Owned { events, slot } = self {
            events.clear();
            slot.recycle(std::mem::take(events));
        }
    }
}

#[cfg(test)]
impl<T> Consumed<'_, T> {
    /// Test-only: inspects the taken buffer's capacity, to check the recycling behavior of `All`/`AllFilter`/
    /// `Cmp`/`Max`/`First`. Not meaningful for `Last`, which never allocates a buffer.
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Owned { events, .. } => events.capacity(),
            Self::Taken(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{SlotType, slot::Slot};

    #[test]
    fn test_consumed_as_slice_shrinks_with_every_consumed_event() {
        let slot = Slot::new(SlotType::All);
        for i in 0..5u32 {
            slot.push(i);
        }

        let mut consumed = slot.consume();
        assert_eq!(consumed.as_slice(), [0, 1, 2, 3, 4]);

        assert_eq!(consumed.next(), Some(0));
        assert_eq!(consumed.as_slice(), [1, 2, 3, 4]);

        assert_eq!(consumed.by_ref().count(), 4);
        assert_eq!(consumed.as_slice().len(), 0);
    }

    #[test]
    fn test_consumed_as_slice_of_a_wrapped_ring_buffer() {
        let slot = Slot::new(SlotType::Max(4));
        for i in 0..10u32 {
            slot.push(i);
        }

        // the ring buffer of a `Max` slot, whose events wrap around the end of its allocation
        let Slot::Store { store, .. } = &slot else {
            unreachable!("`Max` is stored in a ring buffer");
        };
        assert_ne!(
            store.lock().unwrap().as_slices().1.len(),
            0,
            "the ring has to be wrapped"
        );

        // but not the events that are consumed
        let mut consumed = slot.consume();
        assert_eq!(consumed.as_slice(), [6, 7, 8, 9]);

        // consuming from the front keeps them in one piece
        assert_eq!(consumed.next(), Some(6));
        assert_eq!(consumed.next(), Some(7));
        assert_eq!(consumed.as_slice(), [8, 9]);
    }

    #[test]
    fn test_consumed_as_slice_of_max() {
        let slot = Slot::new(SlotType::Max(4));
        for i in 0..10u32 {
            slot.push(i);
        }

        let mut consumed = slot.consume();
        assert_eq!(consumed.as_slice(), [6, 7, 8, 9]);

        consumed.next();
        assert_eq!(consumed.as_slice(), [7, 8, 9]);
    }

    #[test]
    fn test_consumed_as_slice_of_slots_that_hold_one_event() {
        for typ in [SlotType::First, SlotType::Last, SlotType::Cmp(|_, _| true)] {
            let slot = Slot::new(typ);

            slot.push(7u32);
            let mut consumed = slot.consume();
            assert_eq!(consumed.as_slice(), [7]);

            consumed.next();
            assert_eq!(consumed.as_slice().len(), 0);
        }
    }

    #[test]
    fn test_consumed_as_slice_of_nothing() {
        for typ in [
            SlotType::All,
            SlotType::AllFilter(|_| true),
            SlotType::Max(4),
            SlotType::First,
            SlotType::Last,
            SlotType::Cmp(|_, _| true),
        ] {
            let slot = Slot::<u32>::new(typ);
            assert_eq!(slot.consume().as_slice().len(), 0);
        }
    }

    #[test]
    fn test_consumed_knows_how_many_events_are_left() {
        let slot = Slot::new(SlotType::All);
        for i in 0..3u32 {
            slot.push(i);
        }

        let mut consumed = slot.consume();
        assert_eq!(consumed.len(), 3);
        assert!(!consumed.is_empty());
        assert_eq!(consumed.size_hint(), (3, Some(3)));

        consumed.next();
        assert_eq!(consumed.len(), 2);
        assert_eq!(ExactSizeIterator::len(&consumed), 2);

        consumed.by_ref().for_each(drop);
        assert_eq!(consumed.len(), 0);
        assert!(consumed.is_empty());
    }

    #[test]
    fn test_consumed_debug_shows_the_events_that_are_left() {
        let slot = Slot::new(SlotType::All);
        slot.push(1u32);
        slot.push(2);

        let mut consumed = slot.consume();
        assert_eq!(format!("{consumed:?}"), "Consumed([1, 2])");

        consumed.next();
        assert_eq!(format!("{consumed:?}"), "Consumed([2])");
    }
}
