//! The values a tool's arguments carry, in the order its operation's schemas declare them: what
//! a person approving a call is shown. Read from the registry, since the JSON Schemas a tool
//! lists are `serde_json` maps, which keep no order.

use poem_openapi::registry::{MetaSchema, MetaSchemaRef, Registry};

/// Where a tool's argument goes in its operation's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentIn {
    Path,
    Query,
    /// A header: `If-Match` as `expected_version`, `Idempotency-Key` as `idempotency_key`.
    Header,
    Body,
}

/// One of a tool's arguments, and where it is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolArgument {
    pub name: String,
    pub sent: ArgumentIn,
}

/// One value an argument carries: the argument itself, or a property within it that is no
/// object with properties of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolField {
    /// The argument it is in.
    pub argument: String,
    /// The properties, outermost first, it is under within the argument; empty for the
    /// argument itself.
    pub properties: Vec<FieldProperty>,
    /// Its schema's `format`, or, for a list, its items'.
    pub format: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProperty {
    pub name: String,
    /// Its schema's `title`, if it has one.
    pub title: Option<String>,
}

/// The fields of an argument whose schema is `schema`.
pub(super) fn fields_of(
    argument: &str,
    schema: &MetaSchemaRef,
    registry: &Registry,
) -> Vec<ToolField> {
    let mut fields = Vec::new();
    walk(
        argument,
        &mut Vec::new(),
        schema,
        registry,
        &mut Vec::new(),
        &mut fields,
    );
    fields
}

fn walk(
    argument: &str,
    under: &mut Vec<FieldProperty>,
    schema: &MetaSchemaRef,
    registry: &Registry,
    visiting: &mut Vec<String>,
    into: &mut Vec<ToolField>,
) {
    let entered = match schema {
        MetaSchemaRef::Reference(name) if visiting.contains(name) => return,
        MetaSchemaRef::Reference(name) => {
            visiting.push(name.clone());
            true
        }
        MetaSchemaRef::Inline(_) => false,
    };
    let properties = properties(schema, registry);
    if properties.is_empty() {
        into.push(ToolField {
            argument: argument.to_owned(),
            properties: under.clone(),
            format: format(schema, registry),
        });
    } else {
        for (name, property) in properties {
            under.push(FieldProperty {
                name: name.to_owned(),
                title: resolved(property, registry).and_then(|schema| schema.title.clone()),
            });
            walk(argument, under, property, registry, visiting, into);
            under.pop();
        }
    }
    if entered {
        visiting.pop();
    }
}

fn resolved<'a>(schema: &'a MetaSchemaRef, registry: &'a Registry) -> Option<&'a MetaSchema> {
    match schema {
        MetaSchemaRef::Inline(schema) => Some(schema),
        MetaSchemaRef::Reference(name) => registry.schemas.get(name),
    }
}

/// Its own properties, or, as poem-openapi writes a reference it annotates, those of the
/// schemas it is `allOf`.
fn properties<'a>(
    schema: &'a MetaSchemaRef,
    registry: &'a Registry,
) -> Vec<(&'static str, &'a MetaSchemaRef)> {
    let Some(schema) = resolved(schema, registry) else {
        return Vec::new();
    };
    if !schema.properties.is_empty() {
        return schema.properties.iter().map(|(n, s)| (*n, s)).collect();
    }
    schema
        .all_of
        .iter()
        .flat_map(|part| properties(part, registry))
        .collect()
}

fn format(schema: &MetaSchemaRef, registry: &Registry) -> Option<&'static str> {
    let schema = resolved(schema, registry)?;
    schema
        .format
        .or_else(|| {
            schema
                .items
                .as_deref()
                .and_then(|items| format(items, registry))
        })
        .or_else(|| schema.all_of.iter().find_map(|part| format(part, registry)))
}
