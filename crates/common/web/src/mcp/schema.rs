//! Schemas as the OpenAPI spec declares them, read as JSON Schemas: the types
//! they name under JSON Schema's `$defs` rather than `components`, so a tool
//! and its operation describe one type alike.

use poem_openapi::registry::Registry;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const OPENAPI_DEFINITIONS: &str = "#/components/schemas/";
const DEFINITIONS: &str = "#/$defs/";

/// Every type an API names, each referring to the others under `$defs`.
pub(super) struct Definitions(Map<String, Value>);

impl Definitions {
    pub(super) fn of(registry: &Registry) -> Self {
        Self(
            registry
                .schemas
                .iter()
                .map(|(name, schema)| (name.clone(), json_schema(schema)))
                .collect(),
        )
    }

    /// `schema` with the definitions it names, directly or through another,
    /// added under `$defs`, so it stands alone.
    pub(super) fn standalone(&self, mut schema: Value) -> Value {
        let mut named = BTreeSet::new();
        let mut pending = Vec::new();
        references(&schema, &mut pending);
        while let Some(name) = pending.pop() {
            if let Some(definition) = self.0.get(&name)
                && named.insert(name)
            {
                references(definition, &mut pending);
            }
        }
        if !named.is_empty()
            && let Value::Object(object) = &mut schema
        {
            let defs = named
                .into_iter()
                .filter_map(|name| Some((name.clone(), self.0.get(&name)?.clone())))
                .collect();
            object.insert("$defs".into(), Value::Object(defs));
        }
        schema
    }

    /// `schema`, or the definition it refers to: MCP wants a result's schema
    /// to describe an object at its root, not refer to one.
    pub(super) fn resolved(&self, schema: Value) -> Value {
        schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|target| target.strip_prefix(DEFINITIONS))
            .and_then(|name| self.0.get(name))
            .cloned()
            .unwrap_or(schema)
    }
}

/// An OpenAPI schema or reference as a JSON Schema whose references point
/// under `$defs`.
pub(super) fn json_schema(schema: &impl serde::Serialize) -> Value {
    let mut value = serde_json::to_value(schema).expect("an OpenAPI schema serializes");
    retarget(&mut value);
    value
}

fn retarget(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            for (name, value) in object.iter_mut() {
                match (name.as_str(), value) {
                    ("$ref", Value::String(target)) => {
                        *target = target.replace(OPENAPI_DEFINITIONS, DEFINITIONS);
                    }
                    (_, value) => retarget(value),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(retarget),
        _ => {}
    }
}

fn references(schema: &Value, into: &mut Vec<String>) {
    match schema {
        Value::Object(object) => {
            for (name, value) in object {
                match (name.as_str(), value) {
                    ("$ref", Value::String(target)) => {
                        if let Some(name) = target.strip_prefix(DEFINITIONS) {
                            into.push(name.to_owned());
                        }
                    }
                    (_, value) => references(value, into),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| references(item, into)),
        _ => {}
    }
}
