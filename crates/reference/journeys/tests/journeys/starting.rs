use app_authz::PolicyFile;
use app_module::Module;
use server::Composition;

/// The reference modules, notes' default grants naming an action `NoteAction`
/// doesn't have.
struct MisGranted;

impl Composition for MisGranted {
    type Endpoints = <reference_journeys::Reference as Composition>::Endpoints;

    fn modules() -> Vec<Module> {
        let mut modules = reference_journeys::Reference::modules();
        modules[2] = notes::module(users::directory).with_policies(|| {
            notes::policies().default_grants(PolicyFile::new(
                "policies/default_grants.yaml",
                "apiVersion: auth.flagrante.io/v1\ndefaultGrants:\n  - { role: user, resource: note, actions: [create, publish] }\n",
            ))
        });
        modules
    }
}

#[test]
fn a_default_grant_of_an_action_notes_lacks_stops_the_server_naming_it() {
    let refused = server::authorizer::<MisGranted>()
        .expect_err("the server refuses to start")
        .to_string();
    for named in ["`notes`", "policies/default_grants.yaml:3", "`publish`"] {
        assert!(refused.contains(named), "{refused:?} doesn't name {named}");
    }
}
