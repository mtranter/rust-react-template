//! Modules build their own request data (typically a service) so the `.data(..)`
//! wiring lives in each module, not in `main`.

use poem::{Endpoint, EndpointExt};

pub trait Provide {
    type Data: Clone + Send + Sync + 'static;

    fn build(self) -> Self::Data;
}

pub trait Accept: Endpoint + Sized {
    /// Preserves `Output` so the result still composes with `.data`/`.boxed`.
    fn accept<P: Provide>(self, provider: P) -> impl Endpoint<Output = Self::Output> {
        self.data(provider.build())
    }
}

impl<E: Endpoint> Accept for E {}
