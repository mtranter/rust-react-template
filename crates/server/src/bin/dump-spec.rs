//! Needs no database or async runtime (ADR 0006): `Endpoints` are unit structs
//! and services arrive as request data.

fn main() {
    print!("{}", server::build_service().spec());
}
