use super::*;

const NOTE: &str = include_str!("note.yaml");
const SCOPES: &str = include_str!("notes_scopes.yaml");
const DEFAULTS: &str = include_str!("default_grants.yaml");
const LABELS: &str = include_str!("labels.yaml");

#[derive(Clone, Copy)]
enum In {
    Note,
    Scopes,
    Defaults,
    Labels,
}

impl In {
    fn name(self) -> &'static str {
        match self {
            In::Note => "note.yaml",
            In::Scopes => "notes_scopes.yaml",
            In::Defaults => "default_grants.yaml",
            In::Labels => "labels.yaml",
        }
    }
}

/// Loads ADR 0040's files with `from` replaced by `to` in one of them.
fn load_with(file: In, from: &str, to: &str) -> Result<Policies, PolicyError> {
    let edit = |which: In, source: &str| {
        if std::mem::discriminant(&which) == std::mem::discriminant(&file) {
            assert!(source.contains(from), "{} lacks `{from}`", which.name());
            PolicyFile::new(which.name(), source.replacen(from, to, 1))
        } else {
            PolicyFile::new(which.name(), source)
        }
    };
    Policies::builder("notebook")
        .capability(crate::tests::capability())
        .resource::<Note>(edit(In::Note, NOTE))
        .derived_roles(edit(In::Scopes, SCOPES))
        .default_grants(edit(In::Defaults, DEFAULTS))
        .labels(edit(In::Labels, LABELS))
        .build()
}

#[test]
fn the_examples_load_unchanged() {
    load_with(In::Note, "note", "note").expect("loads");
}

#[test]
fn each_refusal_names_the_file_and_line() {
    let rule_end = "      roles: [\"*\"]\n";
    /// What, in which file, replaced by what, refused at which line (`None`: the parser's), why.
    type Case<'a> = (&'a str, In, &'a str, &'a str, Option<usize>, &'a str);
    let cases: &[Case] = &[
        // Checked against the Rust types.
        (
            "an unknown action",
            In::Note,
            "[edit, archive]",
            "[edit, publish]",
            Some(8),
            "`publish` is not an action of `note`",
        ),
        (
            "a wildcard matching no action",
            In::Note,
            "[edit, archive]",
            "[edit, \"tasks:*\"]",
            Some(8),
            "`tasks:*` is not an action of `note`",
        ),
        (
            "an unknown attribute in a rule",
            In::Note,
            "R.attr.archived",
            "R.attr.deleted",
            Some(13),
            "`deleted` is not an attribute of `note`",
        ),
        (
            "an unknown attribute in a derived role",
            In::Scopes,
            "R.attr.authorId",
            "R.attr.ownerId",
            Some(9),
            "`ownerId` is not an attribute of `note`",
        ),
        (
            "an unknown derived role",
            In::Note,
            rule_end,
            "      roles: [\"*\"]\n      derivedRoles: [owner]\n",
            Some(11),
            "`owner` is not a derived role imported",
        ),
        (
            "an unloaded import",
            In::Note,
            "[notes_scopes]",
            "[notes_scopes, other]",
            Some(5),
            "derived roles `other` are not loaded",
        ),
        (
            "a derived role imported twice",
            In::Note,
            "[notes_scopes]",
            "[notes_scopes, notes_scopes]",
            Some(5),
            "derived role `author` is imported twice",
        ),
        (
            "an unknown scope",
            In::Defaults,
            "scope: author",
            "scope: team",
            Some(4),
            "`team` is not a scope of `note`",
        ),
        (
            "a policy for another module's resource",
            In::Note,
            "resource: note",
            "resource: task",
            Some(3),
            "`task` is not a resource given to the `notebook` module's policies",
        ),
        (
            "a policy for the gate given as a resource's",
            In::Note,
            "resource: note",
            "resource: \"module:notebook\"",
            Some(3),
            "`module:notebook` names a Module's gate, and Modules have none (ADR 0047)",
        ),
        (
            "a default grant for another module's resource",
            In::Defaults,
            "resource: note, actions: [create",
            "resource: task, actions: [create",
            Some(3),
            "`task` is not a resource given",
        ),
        (
            "a default grant for a module's gate",
            In::Defaults,
            "resource: note, actions: [create",
            "resource: \"module:notebook\", action: use }\n  - { role: user, resource: note, actions: [create",
            Some(3),
            "`module:notebook` names a Module's gate, and Modules have none (ADR 0047)",
        ),
        (
            "a default grant's unknown action",
            In::Defaults,
            "[create, list]",
            "[create, publish]",
            Some(3),
            "`publish` is not an action of `note`",
        ),
        (
            "a default grant's wildcard other than *",
            In::Defaults,
            "[\"*\"]",
            "[\"items:*\"]",
            Some(5),
            "`items:*` is not an action of `note`",
        ),
        (
            "a default grant with no role",
            In::Defaults,
            "role: user, resource: note, actions: [create",
            "role: \"\", resource: note, actions: [create",
            Some(3),
            "names a role",
        ),
        (
            "a label for another module's resource",
            In::Labels,
            "resource: note",
            "resource: task",
            Some(5),
            "`task` is not a resource given",
        ),
        (
            "a label for a module's gate",
            In::Labels,
            "resource: note",
            "resource: \"module:notebook\"",
            Some(5),
            "`module:notebook` names a Module's gate",
        ),
        (
            "a label for an unknown action",
            In::Labels,
            "create: Write",
            "publish: Write",
            Some(8),
            "`publish` is not an action of `note`",
        ),
        (
            "a label for an unknown scope",
            In::Labels,
            "author: Notes they wrote",
            "team: Their team's",
            Some(12),
            "`team` is not a scope of `note`",
        ),
        (
            "an empty label",
            In::Labels,
            "label: Notes",
            "label: \"\"",
            Some(6),
            "`label` must be some text",
        ),
        (
            "an unknown key in a resource's labels",
            In::Labels,
            "label: Notes",
            "title: Notes",
            Some(6),
            "`title` is not supported in a resource's labels",
        ),
        // Unsupported kinds, fields and expressions.
        (
            "Cerbos's apiVersion",
            In::Note,
            "auth.flagrante.io/v1",
            "api.cerbos.dev/v1",
            Some(1),
            "`apiVersion` must be `auth.flagrante.io/v1`",
        ),
        (
            "a version other than default",
            In::Note,
            "version: default",
            "version: \"20210210\"",
            Some(4),
            "`version` must be `default`",
        ),
        (
            "a principal policy",
            In::Note,
            "resourcePolicy:",
            "principalPolicy:",
            Some(2),
            "`principalPolicy` is not supported in a policy file",
        ),
        (
            "a role policy",
            In::Note,
            "resourcePolicy:",
            "rolePolicy:",
            Some(2),
            "`rolePolicy` is not supported in a policy file",
        ),
        (
            "another kind than the one given",
            In::Note,
            "resourcePolicy:",
            "derivedRoles:",
            Some(2),
            "expected a `resourcePolicy`, found a `derivedRoles`",
        ),
        (
            "exported variables",
            In::Scopes,
            "derivedRoles:",
            "exportVariables:",
            Some(2),
            "`exportVariables` is not supported in a policy file",
        ),
        (
            "a description",
            In::Note,
            "resourcePolicy:",
            "description: notebook\nresourcePolicy:",
            Some(2),
            "`description` is not supported in a policy file",
        ),
        (
            "schemas",
            In::Note,
            "  rules:",
            "  schemas: {}\n  rules:",
            Some(6),
            "`schemas` is not supported in `resourcePolicy`",
        ),
        (
            "policy variables",
            In::Note,
            "  rules:",
            "  variables: {}\n  rules:",
            Some(6),
            "`variables` is not supported in `resourcePolicy`",
        ),
        (
            "a scope field",
            In::Note,
            "  rules:",
            "  scope: acme\n  rules:",
            Some(6),
            "`scope` is not supported in `resourcePolicy`",
        ),
        (
            "rule outputs",
            In::Note,
            rule_end,
            "      roles: [\"*\"]\n      output: {}\n",
            Some(11),
            "`output` is not supported in a rule",
        ),
        (
            "another effect",
            In::Note,
            "EFFECT_DENY",
            "EFFECT_MAYBE",
            Some(9),
            "`effect` is `EFFECT_ALLOW` or `EFFECT_DENY`",
        ),
        (
            "a rule with no roles",
            In::Note,
            rule_end,
            "",
            Some(7),
            "a rule names `roles` or `derivedRoles`",
        ),
        (
            "a rule with no actions",
            In::Note,
            "[edit, archive]",
            "[]",
            Some(8),
            "`actions` lists at least one",
        ),
        (
            "a derived role's unknown field",
            In::Scopes,
            "      parentRoles",
            "      description: x\n      parentRoles",
            Some(6),
            "`description` is not supported in a derived role",
        ),
        (
            "a derived role named any",
            In::Scopes,
            "name: author",
            "name: any",
            Some(5),
            "`any` names every row",
        ),
        (
            "a default grant's unknown field",
            In::Defaults,
            "actions: [create, list] }",
            "actions: [create, list], effect: EFFECT_DENY }",
            Some(3),
            "`effect` is not supported in a default grant",
        ),
        (
            "both action and actions",
            In::Defaults,
            "actions: [create, list] }",
            "action: create, actions: [create, list] }",
            Some(3),
            "one of `action` or `actions`",
        ),
        (
            "a condition's unknown field",
            In::Note,
            "        match:",
            "        when: {}\n        match:",
            Some(12),
            "`when` is not supported in a condition",
        ),
        (
            "a match with two kinds",
            In::Note,
            "          expr: R.attr.archived == true",
            "          expr: R.attr.archived == true\n          all: { of: [] }",
            Some(13),
            "exactly one of",
        ),
        (
            "an empty all",
            In::Note,
            "          expr: R.attr.archived == true",
            "          all: { of: [] }",
            Some(13),
            "`of` lists at least one match",
        ),
        (
            "an unsupported expression",
            In::Note,
            "R.attr.archived == true",
            "size(R.attr.archived) > 1",
            Some(13),
            "is not supported",
        ),
        (
            "a function call",
            In::Note,
            "R.attr.archived == true",
            "R.attr.archived.exists()",
            Some(13),
            "is not supported",
        ),
        (
            "arithmetic",
            In::Note,
            "R.attr.archived == true",
            "R.attr.archived == 1 + 1",
            Some(13),
            "`+` is not supported",
        ),
        (
            "a float",
            In::Note,
            "R.attr.archived == true",
            "R.attr.archived == 1.5",
            Some(13),
            "only integers are supported",
        ),
        (
            "a nested combinator's bad expression",
            In::Note,
            "          expr: R.attr.archived == true",
            "          any:\n            of:\n              - expr: R.attr.archived == true\n              - expr: R.attr.archived > 2",
            Some(16),
            "`>` is not supported",
        ),
        (
            "an expression that isn't a string",
            In::Note,
            "expr: R.attr.archived == true",
            "expr: true",
            Some(13),
            "`expr` must be a string, not a boolean",
        ),
        (
            "a tagged node",
            In::Note,
            "version: default",
            "version: !custom default",
            Some(4),
            "a tagged node",
        ),
        (
            "invalid YAML",
            In::Note,
            "  rules:\n",
            "  rules: [\n",
            None,
            "invalid YAML",
        ),
        (
            "an empty file",
            In::Note,
            NOTE,
            "",
            Some(1),
            "the file is empty",
        ),
        (
            "two documents",
            In::Note,
            NOTE,
            "---\napiVersion: auth.flagrante.io/v1\n---\nresourcePolicy: {}\n",
            Some(4),
            "one YAML document",
        ),
    ];
    for (case, file, from, to, line, reason) in cases {
        let refused = load_with(*file, from, to).expect_err(case);
        assert_eq!(refused.file, file.name(), "{case}: {refused}");
        if let Some(line) = line {
            assert_eq!(refused.line, *line, "{case}: {refused}");
        }
        assert!(refused.reason.contains(reason), "{case}: {refused}");
    }
}

#[test]
fn a_default_grant_naming_a_derived_role_that_isnt_a_scope_is_refused() {
    let refused = load_with(In::Scopes, "[\"*\"]", "[user]").unwrap_err();
    assert_eq!(
        (refused.file.as_str(), refused.line),
        ("default_grants.yaml", 4),
        "{refused}"
    );
    assert!(
        refused
            .reason
            .contains("derived role `author` can't be a scope of `note`"),
        "{refused}"
    );
}

#[test]
fn a_refusal_reads_as_file_line_and_reason() {
    let refused = load_with(In::Note, "[edit, archive]", "[edit, publish]").unwrap_err();
    assert_eq!(
        refused.to_string(),
        "note.yaml:8: `publish` is not an action of `note`"
    );
}

#[test]
fn a_module_naming_no_capability_or_a_malformed_one_is_refused() {
    let without = Policies::builder("notebook")
        .resource::<Note>(PolicyFile::new("note.yaml", NOTE))
        .derived_roles(PolicyFile::new("notes_scopes.yaml", SCOPES))
        .build()
        .unwrap_err();
    assert!(without.reason.contains("names no Capability"), "{without}");

    for (source, why) in [
        ("id: Notes\nlabel: Notes\n", "is not a Capability id"),
        ("id: journal\n", "`label` is required"),
        (
            "id: journal\nlabel: Journal\nowner: me\n",
            "`owner` is not supported",
        ),
    ] {
        let refused = Policies::builder("notebook")
            .capability(PolicyFile::new("capability.yaml", source))
            .resource::<Note>(PolicyFile::new("note.yaml", NOTE))
            .derived_roles(PolicyFile::new("notes_scopes.yaml", SCOPES))
            .build()
            .unwrap_err();
        assert_eq!(refused.file, "capability.yaml");
        assert!(refused.reason.contains(why), "{refused}");
    }
}
