//! The pool's connections, as OpenTelemetry's database client conventions name them.

use opentelemetry::KeyValue;
use opentelemetry::global;
use sqlx::PgPool;

/// The level of the `sqlx::pool::acquire` event `app_web::PoolWaits` reads,
/// below the default log level, so it isn't logged.
pub const ACQUIRE_EVENT_LEVEL: log::LevelFilter = log::LevelFilter::Debug;

/// The one pool each process holds, as its metrics name it.
const POOL_NAME: &str = "app";

/// Observes `pool` on the global meter provider at each collection:
/// `db.client.connection.count` by `db.client.connection.state` (`used`,
/// `idle`), and `db.client.connection.max`. Call it once telemetry is set up;
/// [`connect`](crate::connect) does. The provider keeps `pool` for its life.
pub fn observe_pool(pool: &PgPool) {
    let meter = global::meter("app-db");
    let counted = pool.clone();
    meter
        .i64_observable_up_down_counter("db.client.connection.count")
        .with_unit("{connection}")
        .with_description("The pool's open connections, in use and idle.")
        .with_callback(move |observer| {
            let idle = counted.num_idle() as i64;
            let open = i64::from(counted.size());
            for (state, count) in [("used", open - idle), ("idle", idle)] {
                observer.observe(
                    count,
                    &[
                        KeyValue::new("db.client.connection.pool.name", POOL_NAME),
                        KeyValue::new("db.client.connection.state", state),
                    ],
                );
            }
        })
        .build();
    let max = i64::from(pool.options().get_max_connections());
    meter
        .i64_observable_up_down_counter("db.client.connection.max")
        .with_unit("{connection}")
        .with_description("The most connections the pool opens.")
        .with_callback(move |observer| {
            observer.observe(
                max,
                &[KeyValue::new("db.client.connection.pool.name", POOL_NAME)],
            );
        })
        .build();
}
