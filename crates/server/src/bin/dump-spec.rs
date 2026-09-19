//! Print the OpenAPI spec JSON to stdout. **No database, no async runtime** — the
//! linchpin of the fast inner loop (spec §7). `make` pipes this into
//! `frontend/openapi.json`.

fn main() {
    // `spec()` is synchronous and needs no pool because `Api` structs are unit
    // structs and handlers take the pool via `Data<PgPool>` at request time.
    print!("{}", server::build_service().spec());
}
