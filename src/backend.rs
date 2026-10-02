use std::{panic::RefUnwindSafe, sync::atomic::AtomicBool};

use anythingy::HeapSize;

use crate::{
    err::{EventError, Value},
    map::RegisteredMap,
    query::UnblockingQuery,
    slot::{Slot, SlotType},
};

/// System to register events and event listeners as well as dispatch and query events.
///
///
/// Events can be either handled with an event listener or be registered and then stored and handled in batches later.
///
/// Events can be any type: there is no size restriction, and no wrapping or boxing happens behind your back.
pub struct EventBackend {
    registered: RegisteredMap,
}

impl EventBackend {
    #[must_use]
    /// Creates a new `EventBackend`.
    pub const fn new() -> Self {
        Self {
            registered: RegisteredMap::new(),
        }
    }

    /// Registers a new type of event. Registered events can be queried in a batch.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.register_store::<u32>(SlotType::All);
    /// system.register_store::<(u16, u16)>(SlotType::Last);
    /// # }
    /// ```
    pub fn register_store<T>(&mut self, typ: SlotType<T>)
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.registered.entry::<T>().slot = Some(Box::new(Slot::new(typ)));
    }

    /// Registers a function that gets called, if an event with the matching type is triggered.
    /// Returns the number of listener registered for this type of event.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::EventBackend;
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// let listener = |event: &u32| {
    ///     // handle event
    /// };
    ///
    /// system.register_listener::<u32>(listener);
    /// # }
    /// ```
    pub fn register_listener<T>(
        &mut self,
        listener: impl Fn(&T) + Send + Sync + RefUnwindSafe + 'static,
    ) -> usize
    where
        T: Send + RefUnwindSafe + 'static,
    {
        let registered = self.registered.entry::<T>();
        registered.listener.push(Box::new(listener));
        registered.listener.len()
    }

    /// Triggers a new event, calling all registered event listener. If event was registered to be stored,
    /// event gets saved to be queried later after each listener was called.
    ///
    /// # Errors
    /// Returns an `EventError`, if the event type is not registered, handing the value back.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.register_store::<u32>(SlotType::First);
    ///
    /// system.new_event::<u32>(42).unwrap();
    /// # }
    /// ```
    pub fn new_event<T>(&self, value: T) -> Result<(), EventError<T, Value>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        match self.registered.get::<T>() {
            Some(registered) => {
                registered.handle_event(value);
                Ok(())
            }
            None => Err(EventError::unregistered_event(value)),
        }
    }

    /// Returns an iterator over each event with the matching event type.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventType` error, if the given type was not registered as event type.
    /// Returns an `RegisteredWithoutStore` error, if the queried type is not registered to store events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u32>(SlotType::All);
    /// let query = system.query::<u32>().unwrap();
    ///
    /// for event in query {
    ///     // handle event
    /// }
    /// # }
    /// ```
    pub fn query<T>(&self) -> Result<UnblockingQuery<'_, T>, EventError<T>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.registered.get::<T>().map_or_else(
            || Err(EventError::unregisted_event_empty()),
            |registed| {
                registed.slot().map_or_else(
                    || Err(EventError::registered_without_store()),
                    |slot| Ok(slot.query_owned()),
                )
            },
        )
    }

    /// Disables specific event from being processed.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventTypeError`, if the event type is not registered or no event listener was registered.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u64>(SlotType::All);
    /// system.disable::<u64>().unwrap();
    /// # }
    /// ```
    pub fn disable<T>(&self) -> Result<(), EventError<T>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.registered.get::<T>().map_or_else(
            || Err(EventError::unregisted_event_empty()),
            |registered| {
                registered.disable();
                Ok(())
            },
        )
    }

    /// Disables all events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.disable_all();
    /// # }
    /// ```
    pub fn disable_all(&self) {
        self.registered.disable_all();
    }

    /// Enables specific event for processing.
    ///
    /// # Errors
    /// Returns an `UnregisteredEventTypeError`, if the event type is not registered or no event listener was registered.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// # system.register_store::<u64>(SlotType::Last);
    /// system.enable::<u64>().unwrap();
    /// # }
    /// ```
    pub fn enable<T>(&self) -> Result<(), EventError<T>>
    where
        T: Send + RefUnwindSafe + 'static,
    {
        self.registered.get::<T>().map_or_else(
            || Err(EventError::unregisted_event_empty()),
            |registered| {
                registered.enable();
                Ok(())
            },
        )
    }

    /// Enables all events.
    ///
    /// # Example
    /// ```rust
    /// # use eventsys::{EventBackend, SlotType};
    /// # fn main() {
    /// # let mut system = EventBackend::default();
    /// system.enable_all();
    /// # }
    /// ```
    pub fn enable_all(&self) {
        self.registered.enable_all();
    }

    /// Frees allocated memory for batch events.
    ///
    /// # Warn
    /// All events that are not consumed will get dropped. Also drops all registered listeners.
    pub fn cleanup(&mut self) {
        self.registered.cleanup_all();
    }
}

impl Default for EventBackend {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBackend")
            .field("registered", &self.registered.len())
            .finish()
    }
}

impl HeapSize for EventBackend {
    /// Counts the heap memory every registered event type has allocated: the registration tables, each type's
    /// listeners, and each type's stored events.
    ///
    /// Follows [`HeapSize`]'s own rules: this is what `EventBackend` and its registrations have allocated for
    /// themselves, not what the events or listeners stored inside own in turn (a `Vec<String>` of events counts
    /// the `Vec`'s buffer, not the text each `String` points to), and it is a best-effort snapshot rather than an
    /// exact account, since some of what it walks (an `anythingy::EventQueue`-backed `All`/`AllFilter` slot) is a
    /// snapshot itself.
    #[inline]
    fn heap_size(&self) -> usize {
        self.registered.heap_size()
    }
}

type Listener<T> = Box<dyn Fn(&T) + Sync + RefUnwindSafe + Send>;

pub struct Registered<T> {
    // boxed once at registration, not per event: this keeps `Registered<T>`'s own size independent of `T`, which
    // `RegisteredMap`'s inline type erasure (`anythingy::Thing`) relies on. `Slot<T>` itself is not size-independent
    // of `T` (every variant stores `T` inline, behind a `Mutex` or `EventQueue`).
    slot: Option<Box<Slot<T>>>,
    listener: Vec<Listener<T>>,
    enabled: AtomicBool,
}

impl<T> Registered<T>
where
    T: Send + RefUnwindSafe + 'static,
{
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            slot: None,
            listener: Vec::new(),
            enabled: AtomicBool::new(true),
        }
    }

    /// Calls all listeners with the event and stores it, if a slot is registered.
    pub(crate) fn handle_event(&self, value: T) {
        // check if events for this registered type should be processed
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        // call all listeners
        for listener in &self.listener {
            _ = std::panic::catch_unwind(|| (listener)(&value));
        }

        // store event for querying it later
        if let Some(slot) = &self.slot {
            slot.push(value);
        }
    }

    #[inline]
    pub(crate) fn slot(&self) -> Option<&Slot<T>> {
        self.slot.as_deref()
    }

    #[inline]
    pub(crate) fn enable(&self) {
        self.enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn disable(&self) {
        self.enabled
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn cleanup(&mut self) {
        self.listener = Vec::new();

        if let Some(slot) = &mut self.slot {
            slot.cleanup();
        }
    }

    /// Counts the heap memory this registration has allocated: the listener buffer, the boxed slot itself (sized
    /// like `Box<T>::heap_size`, i.e. the allocation that holds it, not what it owns), and whatever that slot has
    /// allocated beyond that — see [`Slot::heap_size`].
    #[inline]
    pub(crate) fn heap_size(&self) -> usize {
        self.listener.heap_size()
            + self
                .slot
                .as_deref()
                .map_or(0, |slot| size_of_val(slot) + slot.heap_size())
    }
}

impl<T> std::fmt::Debug for Registered<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registered")
            .field("slot", &self.slot.is_some())
            .field("listener", &self.listener.len())
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::EventBackend;

    const fn const_listener<E>(_: &E) {}

    #[test]
    fn test_eventbackend_setup_listener() {
        let mut events = EventBackend::new();

        let count = events.register_listener(const_listener::<u32>);
        assert_eq!(count, 1);

        let count = events.register_listener(const_listener::<u32>);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_eventbackend_setup_register() {
        let mut events = EventBackend::new();

        let count = events.register_listener(const_listener::<u32>);
        assert_eq!(count, 1);

        let count = events.register_listener(const_listener::<u32>);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_event_backend_is_send_sync() {
        // `Registered<T>` gets type-erased into an `SThing`, which only accepts `Send + Sync` values; this guards
        // that `EventBackend` actually stays usable across threads if that ever regresses.
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventBackend>();
    }

    #[test]
    fn test_eventbackend_setup_mixed() {
        let mut events = EventBackend::new();

        let count = events.register_listener::<u32>(const_listener);
        assert_eq!(count, 1);

        events.register_store::<u32>(crate::SlotType::First);
    }

    #[test]
    fn test_heap_size_empty_backend_is_zero() {
        use anythingy::HeapSize;

        assert_eq!(EventBackend::new().heap_size(), 0);
    }

    #[test]
    fn test_heap_size_grows_after_registering_listener() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        assert_eq!(events.heap_size(), 0);

        events.register_listener::<u32>(const_listener);
        // the registration tables (keys/entries) and the listener's own buffer both allocate
        assert!(events.heap_size() > 0);
    }

    #[test]
    fn test_heap_size_single_slot_contributes_nothing_regardless_of_pushes() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        events.register_store::<u32>(crate::SlotType::First);
        let before = events.heap_size();

        for i in 0..100u32 {
            events.new_event(i).unwrap();
        }

        // `Single` (`First`/`Cmp`) stores its value inline in a `Mutex<Option<T>>`, so pushing never allocates
        assert_eq!(events.heap_size(), before);
    }

    #[test]
    fn test_heap_size_last_slot_is_constant_regardless_of_pushes() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        events.register_store::<u32>(crate::SlotType::Last);
        let before = events.heap_size();
        assert!(before > 0, "`AtomicSlot` allocates its two boxes up front");

        for i in 0..1_000u32 {
            events.new_event(i).unwrap();
        }

        // `AtomicSlot` allocates its two boxes once, in `new`; pushing only ever swaps values into them
        assert_eq!(events.heap_size(), before);
    }

    #[test]
    fn test_heap_size_grows_with_stored_all_events() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        events.register_store::<u32>(crate::SlotType::All);
        let before = events.heap_size();

        for i in 0..1_000u32 {
            events.new_event(i).unwrap();
        }

        // the `EventQueue`'s per-thread buffer grows to hold the pushed events
        assert!(events.heap_size() > before);
    }

    #[test]
    fn test_heap_size_grows_with_stored_max_events() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        events.register_store::<u32>(crate::SlotType::Max(1_000));
        let before = events.heap_size();

        for i in 0..1_000u32 {
            events.new_event(i).unwrap();
        }

        // the `Mutex<VecDeque<T>>` buffer grows to hold the pushed events
        assert!(events.heap_size() > before);
    }

    #[test]
    fn test_heap_size_shrinks_after_cleanup() {
        use anythingy::HeapSize;

        let mut events = EventBackend::new();
        events.register_store::<u32>(crate::SlotType::All);
        for i in 0..1_000u32 {
            events.new_event(i).unwrap();
        }
        let before = events.heap_size();

        events.cleanup();

        assert!(events.heap_size() < before);
    }
}
