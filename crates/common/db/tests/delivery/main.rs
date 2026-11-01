//! The worker against Postgres, with small test events: several workers on one
//! database, built from different handlers, stand for builds side by side in a
//! rolling deploy.

mod support;

mod contract;
mod deploys;
mod order;
