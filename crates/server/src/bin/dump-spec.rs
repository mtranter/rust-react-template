//! Needs no database or async runtime (ADR 0006): `Endpoints` are unit structs
//! and services arrive as request data. With `--actions` it prints instead every
//! resource and action the registered modules' policies declare, one
//! `<resource> <action>` per line, which `make check-modules` checks the SPA's
//! Areas against.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--actions") {
        let actions = server::actions::<server::Registered>().unwrap_or_else(|e| {
            eprintln!("the policies don't load: {e}");
            std::process::exit(1)
        });
        for (resource, action) in actions {
            println!("{resource} {action}");
        }
        return;
    }
    print!("{}", server::spec::<server::Registered>());
}
