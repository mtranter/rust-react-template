/// A `Uuid` newtype. No sqlx/serde impls, so domains stay pure. No `new()`:
/// a module adds one only to ids it mints, so referenced ids (e.g. an author
/// owned by another module) can't be invented.
///
/// ```
/// app_domain::uuid_newtype!(
///     /// Identifies a widget.
///     WidgetId
/// );
///
/// impl WidgetId {
///     #[allow(clippy::new_without_default)] // a random `default()` would be misleading
///     pub fn new() -> Self {
///         Self::from_uuid(app_domain::uuid::Uuid::new_v4())
///     }
/// }
/// ```
#[macro_export]
macro_rules! uuid_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name($crate::uuid::Uuid);

        impl $name {
            pub const fn from_uuid(id: $crate::uuid::Uuid) -> Self {
                Self(id)
            }

            pub fn as_uuid(&self) -> $crate::uuid::Uuid {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::uuid::Error;
            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                Ok(Self($crate::uuid::Uuid::parse_str(s)?))
            }
        }

        impl ::std::convert::From<$crate::uuid::Uuid> for $name {
            fn from(id: $crate::uuid::Uuid) -> Self {
                Self(id)
            }
        }
    };
}

/// A seed's readable id, `MMMMMMMM-KKKK-0000-0000-NNNNNNNNNNNN`: module number,
/// entity kind within the module, sequence (spec §13).
///
/// ```
/// let alice = app_domain::well_known_id(1, 1, 1);
/// assert_eq!(alice.to_string(), "00000001-0001-0000-0000-000000000001");
/// ```
pub const fn well_known_id(module: u32, kind: u16, sequence: u64) -> uuid::Uuid {
    assert!(sequence < 1 << 48, "the sequence has 12 hex digits");
    uuid::Uuid::from_u128((module as u128) << 96 | (kind as u128) << 80 | sequence as u128)
}
