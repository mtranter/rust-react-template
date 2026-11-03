mod driver;

// Inline so each test's path starts `journeys::`, which `cargo test -p server
// journeys::` filters on: a test's path leaves out its target's name.
mod journeys {
    mod deleting_accounts;
    mod finding_notes;
}
