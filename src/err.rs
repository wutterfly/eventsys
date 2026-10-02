use std::{any::type_name, error::Error, fmt, marker::PhantomData};

/// Marks an [`EventError`] that carries the event it is about: the event that could not be triggered, which is given
/// back with [`EventError::into_inner`].
#[derive(Debug, Clone, Copy)]
pub struct Value;

/// Marks an [`EventError`] that carries no event.
#[derive(Debug, Clone, Copy)]
pub struct NoValue;

/// The error of an [`EventBackend`](crate::EventBackend) that was asked for something it does not have.
///
/// [`EventError::raw_err`] tells what went wrong, as a [`RawErr`] that can be matched. It is also an
/// [`Error`], and prints what went wrong, so it can be passed on with `?`.
///
/// `V` is [`Value`] if the error carries the event that could not be triggered (see
/// [`EventBackend::new_event`](crate::EventBackend::new_event)), and [`NoValue`] if it does not.
///
/// # Example
/// ```rust
/// use eventsys::{EventBackend, EventError, RawErr, Value};
///
/// fn trigger(events: &EventBackend, key: char) -> Result<(), EventError<char, Value>> {
///     events.new_event(key)
/// }
///
/// // nothing was registered
/// let events = EventBackend::new();
/// let error = trigger(&events, 'h').unwrap_err();
///
/// assert!(matches!(error.raw_err(), RawErr::UnregisteredEventType { .. }));
///
/// // the event that could not be triggered is not lost
/// assert_eq!(error.into_inner(), 'h');
/// ```
pub struct EventError<T: 'static, V = NoValue> {
    inner: Option<T>,
    raw: RawErr,
    v: PhantomData<V>,
}

impl<T: 'static, V> EventError<T, V> {
    /// Returns what went wrong.
    #[inline]
    #[must_use]
    pub const fn raw_err(&self) -> RawErr {
        self.raw
    }
}

impl<T: 'static> EventError<T, Value> {
    /// Returns the event that could not be triggered.
    #[must_use]
    pub fn into_inner(self) -> T {
        debug_assert!(self.inner.is_some());
        // SAFETY:
        // Unwrapping this value is safe, because it is guaranteed with the marker generic Value,
        // that this Option contains a value.
        unsafe { self.inner.unwrap_unchecked() }
    }

    pub(crate) fn unregistered_event(value: T) -> Self {
        Self {
            inner: Some(value),
            v: PhantomData,
            raw: RawErr::UnregisteredEventType {
                type_name: type_name::<T>(),
            },
        }
    }
}

impl<T: 'static> EventError<T, NoValue> {
    pub(crate) fn unregistered_event_empty() -> Self {
        Self {
            inner: None,
            raw: RawErr::UnregisteredEventType {
                type_name: type_name::<T>(),
            },
            v: PhantomData,
        }
    }

    pub(crate) fn registered_without_store() -> Self {
        Self {
            inner: None,
            raw: RawErr::RegisteredWithoutStore {
                type_name: type_name::<T>(),
            },
            v: PhantomData,
        }
    }
}

impl<T: 'static, V> Error for EventError<T, V> {}

impl<T: 'static, V> fmt::Debug for EventError<T, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventError")
            .field("inner", &if self.inner.is_some() { "Some" } else { "None" })
            .field("raw", &self.raw)
            .finish()
    }
}

impl<T: 'static, V> fmt::Display for EventError<T, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.raw, f)
    }
}

/// What went wrong, see [`EventError::raw_err`].
///
/// More kinds can be added in the future, so a `match` needs an arm for the others.
///
/// # Example
/// ```rust
/// use eventsys::{EventBackend, RawErr};
///
/// let mut events = EventBackend::new();
/// events.register_listener::<u32>(|_| {});
///
/// // `u32` has a listener, but nothing that stores its events
/// let error = events.consume::<u32>().err().unwrap();
///
/// match error.raw_err() {
///     RawErr::UnregisteredEventType { .. } => println!("register it first"),
///     RawErr::RegisteredWithoutStore { type_name } => println!("{type_name} has no store"),
///     _ => println!("{error}"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawErr {
    /// The event type was never registered.
    UnregisteredEventType {
        /// The name of the event type.
        type_name: &'static str,
    },

    /// The event type was registered, but not to store events, so there are none to get.
    RegisteredWithoutStore {
        /// The name of the event type.
        type_name: &'static str,
    },
}

impl Error for RawErr {}

impl fmt::Display for RawErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnregisteredEventType { type_name } => {
                write!(f, "Unregistered event type: {type_name}")
            }

            Self::RegisteredWithoutStore { type_name } => {
                write!(
                    f,
                    "Event type was not registered to store events: {type_name}"
                )
            }
        }
    }
}
