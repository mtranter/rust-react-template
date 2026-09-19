//! Module dependency injection.
//!
//! A module contributes its shared request data — typically its service — by
//! implementing [`Provide`] on a small type that carries whatever it needs to
//! build the value (e.g. a pool handle). The server attaches each contribution
//! during assembly with [`Accept::accept`], so the `.data(..)` wiring lives
//! inside the modules instead of `main` reaching into each one.

use poem::{Endpoint, EndpointExt};

/// A module's contribution of shared request data. Handlers extract the built
/// value with `Data<&Self::Data>`.
pub trait Provide {
    /// The value inserted into the app's request data.
    type Data: Clone + Send + Sync + 'static;

    /// Build the value once, at server assembly.
    fn build(self) -> Self::Data;
}

/// Attach a module's [`Provide`]r to the app. Blanket-implemented for every
/// endpoint, so it composes with `.data`, `.with`, and further `.accept` calls.
pub trait Accept: Endpoint + Sized {
    /// Insert `provider`'s data into the app's request data. Preserves the
    /// endpoint's `Output` so the result still composes with `.data`/`.boxed`.
    fn accept<P: Provide>(self, provider: P) -> impl Endpoint<Output = Self::Output> {
        self.data(provider.build())
    }
}

impl<E: Endpoint> Accept for E {}
