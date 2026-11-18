use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};

use crate::PolicyError;

/// A policy file's YAML with every node's line, owned so the checks borrow nothing from saphyr.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub line: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub(crate) enum Kind {
    Str(String),
    Seq(Vec<Node>),
    Map(Vec<(Key, Node)>),
    /// What the policy formats never take where a string, list or mapping is due, described.
    Other(&'static str),
}

impl Kind {
    fn describe(&self) -> &'static str {
        match self {
            Kind::Str(_) => "a string",
            Kind::Seq(_) => "a list",
            Kind::Map(_) => "a mapping",
            Kind::Other(what) => what,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Key {
    pub name: String,
    pub line: usize,
}

/// Names the file in each refusal; a `Node` names the line.
#[derive(Debug, Clone, Copy)]
pub(crate) struct File<'a> {
    pub name: &'a str,
}

impl File<'_> {
    pub fn refuse(&self, line: usize, reason: impl Into<String>) -> PolicyError {
        PolicyError {
            file: self.name.to_owned(),
            line,
            reason: reason.into(),
        }
    }

    pub fn parse(&self, source: &str) -> Result<Node, PolicyError> {
        let mut documents = MarkedYaml::load_from_str(source)
            .map_err(|e| self.refuse(e.marker().line(), format!("invalid YAML: {}", e.info())))?;
        match documents.len() {
            1 => Ok(convert(documents.remove(0))),
            0 => Err(self.refuse(1, "the file is empty")),
            _ => Err(self.refuse(
                documents[1].span.start.line(),
                "a policy file holds one YAML document",
            )),
        }
    }

    pub fn map<'n>(&self, node: &'n Node, what: &str) -> Result<Fields<'n>, PolicyError> {
        match &node.kind {
            Kind::Map(entries) => Ok(Fields {
                line: node.line,
                entries,
                taken: vec![false; entries.len()],
            }),
            _ => Err(self.refuse(
                node.line,
                format!("{what} must be a mapping, not {}", node.kind.describe()),
            )),
        }
    }

    pub fn seq<'n>(&self, node: &'n Node, what: &str) -> Result<&'n [Node], PolicyError> {
        match &node.kind {
            Kind::Seq(items) => Ok(items),
            _ => Err(self.refuse(
                node.line,
                format!("{what} must be a list, not {}", node.kind.describe()),
            )),
        }
    }

    pub fn str<'n>(&self, node: &'n Node, what: &str) -> Result<&'n str, PolicyError> {
        match &node.kind {
            Kind::Str(s) => Ok(s),
            _ => Err(self.refuse(
                node.line,
                format!("{what} must be a string, not {}", node.kind.describe()),
            )),
        }
    }

    pub fn strings(&self, node: &Node, what: &str) -> Result<Vec<(String, usize)>, PolicyError> {
        self.seq(node, what)?
            .iter()
            .map(|item| Ok((self.str(item, what)?.to_owned(), item.line)))
            .collect()
    }
}

/// A mapping read field by field; `finish` refuses whatever wasn't read.
pub(crate) struct Fields<'n> {
    pub line: usize,
    entries: &'n [(Key, Node)],
    taken: Vec<bool>,
}

impl<'n> Fields<'n> {
    pub fn has(&self, name: &str) -> bool {
        self.key_line(name).is_some()
    }

    pub fn key_line(&self, name: &str) -> Option<usize> {
        self.entries
            .iter()
            .find(|(key, _)| key.name == name)
            .map(|(key, _)| key.line)
    }

    pub fn take(&mut self, name: &str) -> Option<&'n Node> {
        let index = self.entries.iter().position(|(key, _)| key.name == name)?;
        self.taken[index] = true;
        Some(&self.entries[index].1)
    }

    pub fn require(&mut self, file: &File, name: &str) -> Result<&'n Node, PolicyError> {
        let line = self.line;
        self.take(name)
            .ok_or_else(|| file.refuse(line, format!("`{name}` is required")))
    }

    pub fn finish(self, file: &File, within: &str) -> Result<(), PolicyError> {
        match self
            .entries
            .iter()
            .zip(&self.taken)
            .find(|(_, taken)| !**taken)
        {
            Some(((key, _), _)) => Err(file.refuse(
                key.line,
                format!("`{}` is not supported in {within}", key.name),
            )),
            None => Ok(()),
        }
    }
}

fn convert(node: MarkedYaml) -> Node {
    let line = node.span.start.line();
    let kind = match node.data {
        YamlData::Value(Scalar::Null) => Kind::Other("null"),
        YamlData::Value(Scalar::Boolean(_)) => Kind::Other("a boolean"),
        YamlData::Value(Scalar::Integer(_)) => Kind::Other("an integer"),
        YamlData::Value(Scalar::String(s)) => Kind::Str(s.into_owned()),
        YamlData::Value(Scalar::FloatingPoint(_)) => Kind::Other("a number"),
        YamlData::Sequence(items) => Kind::Seq(items.into_iter().map(convert).collect()),
        YamlData::Mapping(entries) => Kind::Map(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let key_line = key.span.start.line();
                    let name = match key.data {
                        YamlData::Value(Scalar::String(s)) => s.into_owned(),
                        _ => String::new(),
                    };
                    (
                        Key {
                            name,
                            line: key_line,
                        },
                        convert(value),
                    )
                })
                .collect(),
        ),
        YamlData::Tagged(..) => Kind::Other("a tagged node, which is not supported"),
        YamlData::Alias(_) => Kind::Other("an alias, which is not supported"),
        YamlData::Representation(..) | YamlData::BadValue => Kind::Other("an unreadable value"),
    };
    Node { line, kind }
}
