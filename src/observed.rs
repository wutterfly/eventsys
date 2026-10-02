use std::fmt;

/// An iterator over events of type `T`, which borrows them, see
/// [`EventBackend::observe`](crate::EventBackend::observe).
///
/// The events were observed, so the event system keeps them, and any number of observers can look at the same ones.
/// Every call to `observe` returns its own `Observed`, so iterating one does not affect any other.
///
/// The counterpart of [`Consumed`](crate::Consumed), which owns the events it was handed. Both are used the same way:
/// they iterate over the events (`&T` for `Observed`, `T` for `Consumed`), know how many are left, and can show them as
/// a slice.
///
/// Like any iterator that keeps track of where it is, it is neither `Clone` nor `Copy`. Call `observe` again to look at
/// the events a second time.
///
/// ```compile_fail
/// fn assert_clone<T: Clone>() {}
/// assert_clone::<eventsys::Observed<u32>>();
/// ```
///
/// ```compile_fail
/// fn assert_copy<T: Copy>() {}
/// assert_copy::<eventsys::Observed<u32>>();
/// ```
pub struct Observed<'a, T>(&'a [T]);

impl<'a, T> Observed<'a, T> {
    #[inline]
    pub(crate) const fn new(events: &'a [T]) -> Self {
        Self(events)
    }

    /// Returns the number of events this `Observed` can produce.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The events that were not iterated over yet, as a slice, in the order they are produced in. Every event that
    /// is produced is removed from the front of it.
    ///
    /// The slice lives as long as the borrow of the event system, not as long as this `Observed`.
    #[inline]
    #[must_use]
    pub const fn as_slice(&self) -> &'a [T] {
        self.0
    }
}

impl<'a, T> Iterator for Observed<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let (first, rest) = self.0.split_first()?;
        self.0 = rest;

        Some(first)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for Observed<'_, T> {}

impl<T: fmt::Debug> fmt::Debug for Observed<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Observed").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Observed;

    #[test]
    fn test_observed_iterates_over_references() {
        let events = [1, 2, 3];
        let mut observed = Observed::new(&events);

        assert_eq!(observed.next(), Some(&1));
        assert_eq!(observed.next(), Some(&2));
        assert_eq!(observed.next(), Some(&3));
        assert_eq!(observed.next(), None);
        assert_eq!(observed.next(), None);
    }

    #[test]
    fn test_observed_works_in_a_for_loop() {
        let events = [1, 2, 3];

        let mut seen = Vec::new();
        for event in Observed::new(&events) {
            seen.push(*event);
        }

        assert_eq!(seen, [1, 2, 3]);
        assert_eq!(Observed::new(&events).copied().sum::<i32>(), 6);
    }

    #[test]
    fn test_observed_knows_how_many_events_are_left() {
        let events = [1, 2, 3];
        let mut observed = Observed::new(&events);

        assert_eq!(observed.len(), 3);
        assert!(!observed.is_empty());
        assert_eq!(observed.size_hint(), (3, Some(3)));

        observed.next();
        assert_eq!(observed.len(), 2);
        assert_eq!(ExactSizeIterator::len(&observed), 2);

        observed.by_ref().for_each(drop);
        assert_eq!(observed.len(), 0);
        assert!(observed.is_empty());
    }

    #[test]
    fn test_observed_as_slice_shrinks_with_every_event() {
        let events = [1, 2, 3];
        let mut observed = Observed::new(&events);

        assert_eq!(observed.as_slice(), [1, 2, 3]);

        observed.next();
        assert_eq!(observed.as_slice(), [2, 3]);

        observed.by_ref().for_each(drop);
        assert_eq!(observed.as_slice().len(), 0);
    }

    #[test]
    fn test_observed_as_slice_outlives_the_observed() {
        let events = [1, 2];

        let slice = {
            let observed = Observed::new(&events);
            observed.as_slice()
        };

        assert_eq!(slice, [1, 2]);
    }

    #[test]
    fn test_observed_debug_shows_the_events_that_are_left() {
        let events = [1, 2];
        let mut observed = Observed::new(&events);
        assert_eq!(format!("{observed:?}"), "Observed([1, 2])");

        observed.next();
        assert_eq!(format!("{observed:?}"), "Observed([2])");
    }
}
