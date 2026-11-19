//! An in-memory metric exporter, for a test to check what its requests measure.

use super::traces::undecided;
use crate::PoolWaits;
use opentelemetry::global;
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
use std::collections::BTreeSet;
use std::sync::OnceLock;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;

/// Every metric recorded in the process while it is installed. The meter
/// provider is global, so this one is shared by every test of the binary
/// and never taken down: check a metric is there, not what it adds up to.
/// Install it before building what records into it (the app, the pool's
/// `app_db::observe_pool`). It also measures this thread's pool waits.
pub struct Metrics {
    provider: SdkMeterProvider,
    exporter: InMemoryMetricExporter,
    _installed: DefaultGuard,
}

impl Metrics {
    pub fn install() -> Self {
        static SHARED: OnceLock<(SdkMeterProvider, InMemoryMetricExporter)> = OnceLock::new();
        let (provider, exporter) = SHARED
            .get_or_init(|| {
                let exporter = InMemoryMetricExporter::default();
                let provider = SdkMeterProvider::builder()
                    .with_reader(PeriodicReader::builder(exporter.clone()).build())
                    .build();
                global::set_meter_provider(provider.clone());
                (provider, exporter)
            })
            .clone();
        undecided();
        let subscriber =
            tracing_subscriber::registry().with(PoolWaits::layer(&global::meter("app-web")));
        Self {
            provider,
            exporter,
            _installed: tracing::subscriber::set_default(subscriber),
        }
    }

    /// The name of every metric with a measurement so far.
    pub fn names(&self) -> BTreeSet<String> {
        self.provider
            .force_flush()
            .expect("the in-memory exporter takes every collection");
        self.exporter
            .get_finished_metrics()
            .expect("the in-memory exporter is never shut down")
            .iter()
            .flat_map(|resource| resource.scope_metrics())
            .flat_map(|scope| scope.metrics())
            .map(|metric| metric.name().to_owned())
            .collect()
    }
}
