//! What a write awaiting the owner's approval would do, in their terms rather than the tool's:
//! built from its operation and its stored arguments, never from the model's words. Approval is
//! the defence against prompt injection (Spec 0012), so what the owner is shown must be exactly
//! the call that approving runs.

use super::{AppTool, AppTools, ArgumentIn, ToolField};
use crate::domain::{Conversation, ToolCallId};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The first of a record's properties holding a non-empty string names it.
const NAMED_BY: [&str; 5] = ["title", "name", "displayName", "email", "subject"];
/// The formats a client shows in the owner's locale.
const DATES: [&str; 2] = ["date", "date-time"];

#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The operation, as words: `edit_note` is "Edit note".
    pub action: String,
    /// The record it acts on, when one can be read and named.
    pub target: Option<Target>,
    /// What it sends, in its schemas' order.
    pub fields: Vec<ProposedField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// What the record is, from the read that found it: `get_note` reads a "Note".
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProposedField {
    pub label: String,
    pub value: Value,
    /// `date` or `date-time`, when its schema says so.
    pub format: Option<String>,
    /// The target's value of it, when the call changes it.
    pub previous: Option<Value>,
}

/// The proposal of each call of `conversation` awaiting the owner's approval, reading each
/// target through `tools`, as the owner.
pub async fn proposals(
    conversation: &Conversation,
    tools: &dyn AppTools,
) -> HashMap<ToolCallId, Proposal> {
    let surface = tools.tools();
    let mut proposals = HashMap::new();
    for call in conversation.awaiting_approval() {
        if let Some(proposal) = propose(tools, &surface, call.tool(), call.arguments()).await {
            proposals.insert(call.id(), proposal);
        }
    }
    proposals
}

/// What calling `tool` with `arguments` would do; nothing for a tool not on `surface`.
pub async fn propose(
    tools: &dyn AppTools,
    surface: &[AppTool],
    tool: &str,
    arguments: &Value,
) -> Option<Proposal> {
    let tool = surface.iter().find(|candidate| candidate.name == tool)?;
    let record = match target_read(tool, surface, arguments) {
        Some((read, with)) => {
            let answered = tools.call(&read.name, with, None).await;
            answered.succeeded().then_some((read, answered.body))
        }
        None => None,
    };
    let target = record.as_ref().and_then(|(read, body)| {
        Some(Target {
            kind: kind_of(&read.name),
            name: name_of(body)?,
        })
    });
    Some(Proposal {
        action: sentence_case(&tool.name),
        target,
        fields: fields(tool, arguments, record.as_ref().map(|(_, body)| body)),
    })
}

/// The read of the record `tool` acts on, and its arguments: a `GET` at the same path
/// template, or at the longest prefix of it that still names a record by a path parameter.
fn target_read<'s>(
    tool: &AppTool,
    surface: &'s [AppTool],
    arguments: &Value,
) -> Option<(&'s AppTool, Value)> {
    let segments: Vec<&str> = tool.path.split('/').filter(|s| !s.is_empty()).collect();
    (1..=segments.len()).rev().find_map(|length| {
        let prefix = &segments[..length];
        let parameters: Vec<&str> = prefix.iter().filter_map(|s| parameter(s)).collect();
        if parameters.is_empty() {
            return None;
        }
        let path = format!("/{}", prefix.join("/"));
        let read = surface
            .iter()
            .find(|candidate| candidate.reads && candidate.path == path)?;
        let mut with = Map::new();
        for name in parameters {
            with.insert(name.to_owned(), arguments.get(name)?.clone());
        }
        Some((read, Value::Object(with)))
    })
}

fn parameter(segment: &str) -> Option<&str> {
    segment.strip_prefix('{')?.strip_suffix('}')
}

fn name_of(record: &Value) -> Option<String> {
    NAMED_BY.iter().find_map(|property| {
        record
            .get(property)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    })
}

/// `get_note` reads a "Note": its operation id without the verb.
fn kind_of(read: &str) -> String {
    sentence_case(read.split_once('_').map_or(read, |(_, kind)| kind))
}

/// The business data `arguments` send: neither headers (`expected_version`,
/// `idempotency_key`), nor path parameters (the target), nor versions or ids.
fn fields(tool: &AppTool, arguments: &Value, record: Option<&Value>) -> Vec<ProposedField> {
    let shown = |argument: &str| {
        tool.arguments.iter().any(|candidate| {
            candidate.name == argument
                && matches!(candidate.sent, ArgumentIn::Query | ArgumentIn::Body)
        })
    };
    let mut fields = Vec::new();
    for field in tool.fields.iter().filter(|field| shown(&field.argument)) {
        if field.format.as_deref() == Some("uuid") {
            continue;
        }
        let at = within(field);
        if at.last().is_some_and(|name| name == "version") {
            continue;
        }
        let Some(value) = lookup(arguments.get(&field.argument), &at) else {
            continue;
        };
        let labels = if field.properties.is_empty() {
            if field.argument == "body" {
                vec![]
            } else {
                vec![sentence_case(&field.argument)]
            }
        } else {
            field
                .properties
                .iter()
                .map(|property| {
                    property
                        .title
                        .clone()
                        .unwrap_or_else(|| sentence_case(&property.name))
                })
                .collect()
        };
        let format = field
            .format
            .clone()
            .filter(|format| DATES.contains(&format.as_str()));
        push(&mut fields, labels, at, value, format, record);
    }
    fields
}

/// Where `field` is in a record: a body's property where it is in the body, a query
/// argument by its name.
fn within(field: &ToolField) -> Vec<String> {
    if field.properties.is_empty() {
        if field.argument == "body" {
            vec![]
        } else {
            vec![field.argument.clone()]
        }
    } else {
        field.properties.iter().map(|p| p.name.clone()).collect()
    }
}

fn lookup<'v>(value: Option<&'v Value>, at: &[String]) -> Option<&'v Value> {
    at.iter()
        .try_fold(value?, |value, name| value.get(name.as_str()))
}

/// `value` as a field, or, an object its schema doesn't describe, each of its properties.
fn push(
    fields: &mut Vec<ProposedField>,
    labels: Vec<String>,
    at: Vec<String>,
    value: &Value,
    format: Option<String>,
    record: Option<&Value>,
) {
    if let Value::Object(properties) = value
        && !properties.is_empty()
    {
        for (name, value) in properties.iter().filter(|(name, _)| *name != "version") {
            let mut labels = labels.clone();
            labels.push(sentence_case(name));
            let mut at = at.clone();
            at.push(name.clone());
            push(fields, labels, at, value, None, record);
        }
        return;
    }
    let previous = if at.is_empty() {
        None
    } else {
        lookup(record, &at).filter(|was| *was != value).cloned()
    };
    fields.push(ProposedField {
        label: if labels.is_empty() {
            "Details".to_owned()
        } else {
            labels.join(" › ")
        },
        value: value.clone(),
        format,
        previous,
    });
}

/// `edit_note` and `archiveAt` as "Edit note" and "Archive at".
pub fn sentence_case(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            if c.is_uppercase()
                && previous.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                && !word.is_empty()
            {
                words.push(std::mem::take(&mut word));
            }
            word.extend(c.to_lowercase());
        }
        previous = Some(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    let joined = words.join(" ");
    let mut chars = joined.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => joined,
    }
}

#[cfg(test)]
mod tests {
    use super::sentence_case;

    #[test]
    fn names_read_in_sentence_case() {
        assert_eq!(sentence_case("edit_note"), "Edit note");
        assert_eq!(sentence_case("assign_role"), "Assign role");
        assert_eq!(sentence_case("archiveAt"), "Archive at");
        assert_eq!(sentence_case("displayName"), "Display name");
        assert_eq!(sentence_case("note"), "Note");
    }
}
