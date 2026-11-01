mod clock;
mod ids;

pub use clock::{now, seed_time};
pub use ids::well_known_id;

// Re-exported so `uuid_newtype!` expands without callers depending on `uuid`.
#[doc(hidden)]
pub use uuid;
