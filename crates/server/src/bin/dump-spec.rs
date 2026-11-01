//! Needs no database or async runtime (spec §7): `Endpoints` are unit structs
//! and services arrive as request data.

fn main() {
    print!("{}", server::build_service().spec());
}
