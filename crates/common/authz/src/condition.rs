use std::fmt;

use crate::yaml::{File, Node};
use crate::{PolicyError, Principal, Resource, Value};

/// A policy's condition, before a principal is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Condition {
    Compare {
        attr: String,
        op: Op,
        operand: Operand,
    },
    In {
        attr: String,
        values: Vec<Value>,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    None(Vec<Condition>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Operand {
    Literal(Value),
    PrincipalId,
    PrincipalAttr(String),
}

/// What is left of the conditions once the principal is known: over the resource's attributes
/// alone, so it can be checked against one row (`matches`) or become a query's `WHERE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    True,
    False,
    Eq(String, Value),
    Ne(String, Value),
    In(String, Vec<Value>),
    All(Vec<Filter>),
    Any(Vec<Filter>),
    None(Vec<Filter>),
}

impl Filter {
    /// A missing attribute makes a comparison false, whatever its operator, as SQL's `NULL` would.
    pub fn matches<R: Resource>(&self, resource: &R) -> bool {
        match self {
            Filter::True => true,
            Filter::False => false,
            Filter::Eq(attr, value) => resource.attr(attr).is_some_and(|v| &v == value),
            Filter::Ne(attr, value) => resource.attr(attr).is_some_and(|v| &v != value),
            Filter::In(attr, values) => resource.attr(attr).is_some_and(|v| values.contains(&v)),
            Filter::All(fs) => fs.iter().all(|f| f.matches(resource)),
            Filter::Any(fs) => fs.iter().any(|f| f.matches(resource)),
            Filter::None(fs) => !fs.iter().any(|f| f.matches(resource)),
        }
    }

    pub(crate) fn all(filters: Vec<Filter>) -> Filter {
        let mut kept = Vec::new();
        for f in filters {
            match f {
                Filter::True => {}
                Filter::False => return Filter::False,
                Filter::All(inner) => kept.extend(inner),
                f => kept.push(f),
            }
        }
        dedup(&mut kept);
        absorb(&mut kept, |f| match f {
            Filter::Any(xs) => Some(xs),
            _ => None,
        });
        match kept.len() {
            0 => Filter::True,
            1 => kept.remove(0),
            _ => Filter::All(kept),
        }
    }

    pub(crate) fn any(filters: Vec<Filter>) -> Filter {
        let mut kept = Vec::new();
        for f in filters {
            match f {
                Filter::False => {}
                Filter::True => return Filter::True,
                Filter::Any(inner) => kept.extend(inner),
                f => kept.push(f),
            }
        }
        dedup(&mut kept);
        absorb(&mut kept, |f| match f {
            Filter::All(xs) => Some(xs),
            _ => None,
        });
        match kept.len() {
            0 => Filter::False,
            1 => kept.remove(0),
            _ => Filter::Any(kept),
        }
    }

    pub(crate) fn none(filters: Vec<Filter>) -> Filter {
        match Filter::any(filters) {
            Filter::True => Filter::False,
            Filter::False => Filter::True,
            Filter::Any(fs) => Filter::None(fs),
            f => Filter::None(vec![f]),
        }
    }
}

fn dedup(filters: &mut Vec<Filter>) {
    let mut seen = Vec::with_capacity(filters.len());
    filters.retain(|f| {
        let fresh = !seen.contains(f);
        if fresh {
            seen.push(f.clone());
        }
        fresh
    });
}

/// Absorption: `a or (a and b)` is `a`, and `a and (a or b)` is `a`. `inner` picks the terms of
/// the other combinator, which an operand is dropped for when one of them is also an operand.
fn absorb(filters: &mut Vec<Filter>, inner: impl Fn(&Filter) -> Option<&Vec<Filter>>) {
    let absorbed: Vec<bool> = filters
        .iter()
        .enumerate()
        .map(|(i, f)| {
            inner(f).is_some_and(|terms| {
                filters
                    .iter()
                    .enumerate()
                    .any(|(j, g)| i != j && terms.contains(g))
            })
        })
        .collect();
    let mut index = 0;
    filters.retain(|_| {
        index += 1;
        !absorbed[index - 1]
    });
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |f: &mut fmt::Formatter<'_>, name: &str, fs: &[Filter]| {
            write!(f, "{name}(")?;
            for (i, x) in fs.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{x}")?;
            }
            f.write_str(")")
        };
        match self {
            Filter::True => f.write_str("true"),
            Filter::False => f.write_str("false"),
            Filter::Eq(a, v) => write!(f, "{a} == {v}"),
            Filter::Ne(a, v) => write!(f, "{a} != {v}"),
            Filter::In(a, vs) => {
                write!(f, "{a} in [")?;
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
            Filter::All(fs) => list(f, "all", fs),
            Filter::Any(fs) => list(f, "any", fs),
            Filter::None(fs) => list(f, "none", fs),
        }
    }
}

impl Condition {
    /// Substitutes the principal. A `P.attr` the principal lacks makes its comparison false.
    pub(crate) fn resolve(&self, principal: &Principal) -> Filter {
        let Principal::User {
            id, attr: pattr, ..
        } = principal
        else {
            return Filter::True;
        };
        match self {
            Condition::Compare { attr, op, operand } => {
                let value = match operand {
                    Operand::Literal(v) => v.clone(),
                    Operand::PrincipalId => Value::String(id.clone()),
                    Operand::PrincipalAttr(name) => match pattr.get(name) {
                        Some(v) => v.clone(),
                        None => return Filter::False,
                    },
                };
                match op {
                    Op::Eq => Filter::Eq(attr.clone(), value),
                    Op::Ne => Filter::Ne(attr.clone(), value),
                }
            }
            Condition::In { attr, values } => Filter::In(attr.clone(), values.clone()),
            Condition::All(cs) => Filter::all(cs.iter().map(|c| c.resolve(principal)).collect()),
            Condition::Any(cs) => Filter::any(cs.iter().map(|c| c.resolve(principal)).collect()),
            Condition::None(cs) => Filter::none(cs.iter().map(|c| c.resolve(principal)).collect()),
        }
    }

    pub(crate) fn attrs(&self, out: &mut Vec<String>) {
        match self {
            Condition::Compare { attr, .. } | Condition::In { attr, .. } => out.push(attr.clone()),
            Condition::All(cs) | Condition::Any(cs) | Condition::None(cs) => {
                cs.iter().for_each(|c| c.attrs(out))
            }
        }
    }
}

/// A located condition, so a check against a resource's attributes can name the line.
#[derive(Debug, Clone)]
pub(crate) struct Located {
    pub condition: Condition,
    /// Each attribute named, with the line of the expression naming it.
    pub attrs: Vec<(String, usize)>,
}

impl Located {
    pub fn check_attrs(&self, file: &str, kind: &str, known: &[&str]) -> Result<(), PolicyError> {
        match self
            .attrs
            .iter()
            .find(|(a, _)| !known.contains(&a.as_str()))
        {
            Some((attr, line)) => Err(PolicyError {
                file: file.to_owned(),
                line: *line,
                reason: format!("`{attr}` is not an attribute of `{kind}`"),
            }),
            None => Ok(()),
        }
    }
}

/// `condition: { match: <match> }`.
pub(crate) fn parse_condition(file: &File, node: &Node) -> Result<Located, PolicyError> {
    let mut fields = file.map(node, "a condition")?;
    let m = fields.require(file, "match")?;
    fields.finish(file, "a condition")?;
    let mut attrs = Vec::new();
    let condition = parse_match(file, m, &mut attrs)?;
    Ok(Located { condition, attrs })
}

/// `expr: <expression>`, or one of `all`, `any`, `none`, each `of:` a list of matches.
fn parse_match(
    file: &File,
    node: &Node,
    attrs: &mut Vec<(String, usize)>,
) -> Result<Condition, PolicyError> {
    let mut fields = file.map(node, "a match")?;
    let present: Vec<&str> = ["expr", "all", "any", "none"]
        .into_iter()
        .filter(|k| fields.has(k))
        .collect();
    let condition = match present.as_slice() {
        ["expr"] => {
            let expr = fields.require(file, "expr")?;
            let source = file.str(expr, "`expr`")?;
            let condition = parse_expr(source).map_err(|reason| file.refuse(expr.line, reason))?;
            let mut names = Vec::new();
            condition.attrs(&mut names);
            attrs.extend(names.into_iter().map(|a| (a, expr.line)));
            condition
        }
        [combinator] => {
            let body = fields.require(file, combinator)?;
            let mut inner = file.map(body, &format!("`{combinator}`"))?;
            let of = inner.require(file, "of")?;
            inner.finish(file, &format!("`{combinator}`"))?;
            let items = file.seq(of, "`of`")?;
            if items.is_empty() {
                return Err(file.refuse(of.line, "`of` lists at least one match"));
            }
            let parsed = items
                .iter()
                .map(|item| parse_match(file, item, attrs))
                .collect::<Result<Vec<_>, _>>()?;
            match *combinator {
                "all" => Condition::All(parsed),
                "any" => Condition::Any(parsed),
                _ => Condition::None(parsed),
            }
        }
        [] => {
            fields.finish(file, "a match")?;
            return Err(file.refuse(
                node.line,
                "a match needs one of `expr`, `all`, `any` or `none`",
            ));
        }
        _ => {
            return Err(file.refuse(
                node.line,
                "a match has exactly one of `expr`, `all`, `any` or `none`",
            ));
        }
    };
    fields.finish(file, "a match")?;
    Ok(condition)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Path(String),
    Eq,
    Ne,
    In,
    Open,
    Close,
    Comma,
    Literal(Value),
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '[' => {
                tokens.push(Token::Open);
                i += 1;
            }
            ']' => {
                tokens.push(Token::Close);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '=' | '!' if chars.get(i + 1) == Some(&'=') => {
                tokens.push(if c == '=' { Token::Eq } else { Token::Ne });
                i += 2;
            }
            '"' | '\'' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err("unterminated string".into()),
                        Some(&q) if q == c => break,
                        Some('\\') => {
                            match chars.get(i + 1) {
                                Some(&e @ ('\\' | '"' | '\'')) => s.push(e),
                                _ => return Err("unsupported escape in a string".into()),
                            }
                            i += 1;
                        }
                        Some(&ch) => s.push(ch),
                    }
                    i += 1;
                }
                tokens.push(Token::Literal(Value::String(s)));
                i += 1;
            }
            c if c.is_ascii_digit() || c == '-' => {
                let start = i;
                i += 1;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '.')
                {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n = text.parse::<i64>().map_err(|_| {
                    format!("`{text}` is not an integer (only integers are supported)")
                })?;
                tokens.push(Token::Literal(Value::Integer(n)));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                tokens.push(match word.as_str() {
                    "in" => Token::In,
                    "true" => Token::Literal(Value::Boolean(true)),
                    "false" => Token::Literal(Value::Boolean(false)),
                    "null" => Token::Literal(Value::Null),
                    _ => Token::Path(word),
                });
            }
            c => return Err(format!("`{c}` is not supported in an expression")),
        }
    }
    Ok(tokens)
}

enum Side {
    Attr(String),
    Operand(Operand),
}

fn side(token: &Token) -> Result<Side, String> {
    match token {
        Token::Literal(v) => Ok(Side::Operand(Operand::Literal(v.clone()))),
        Token::Path(p) => {
            if let Some(a) = p.strip_prefix("R.attr.").filter(|a| is_name(a)) {
                Ok(Side::Attr(a.to_owned()))
            } else if p == "P.id" {
                Ok(Side::Operand(Operand::PrincipalId))
            } else if let Some(a) = p.strip_prefix("P.attr.").filter(|a| is_name(a)) {
                Ok(Side::Operand(Operand::PrincipalAttr(a.to_owned())))
            } else {
                Err(format!(
                    "`{p}` is not supported: use `R.attr.<name>`, `P.id`, `P.attr.<name>` or a literal"
                ))
            }
        }
        _ => Err(UNSUPPORTED.into()),
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && !s.contains('.')
}

const UNSUPPORTED: &str = "unsupported expression: compare `R.attr.<name>` with `==` or `!=` to `P.id`, \
     `P.attr.<name>` or a literal, or use `R.attr.<name> in [<literals>]`";

/// The subset of CEL in ADR 0040: one comparison or one `in`, and nothing else.
pub(crate) fn parse_expr(source: &str) -> Result<Condition, String> {
    let tokens = tokenize(source)?;
    match tokens.as_slice() {
        [left, op @ (Token::Eq | Token::Ne), right] => {
            let op = if *op == Token::Eq { Op::Eq } else { Op::Ne };
            match (side(left)?, side(right)?) {
                (Side::Attr(attr), Side::Operand(operand))
                | (Side::Operand(operand), Side::Attr(attr)) => {
                    Ok(Condition::Compare { attr, op, operand })
                }
                (Side::Attr(_), Side::Attr(_)) => {
                    Err("comparing two resource attributes is not supported".into())
                }
                (Side::Operand(_), Side::Operand(_)) => {
                    Err("an expression compares a resource attribute, `R.attr.<name>`".into())
                }
            }
        }
        [left, Token::In, Token::Open, rest @ .., Token::Close] => {
            let Side::Attr(attr) = side(left)? else {
                return Err("only `R.attr.<name>` may be tested with `in`".into());
            };
            let mut values = Vec::new();
            for (i, token) in rest.iter().enumerate() {
                match (i % 2, token) {
                    (0, Token::Literal(v)) => values.push(v.clone()),
                    (1, Token::Comma) => {}
                    _ => return Err("`in` takes a list of literals".into()),
                }
            }
            if values.is_empty() {
                return Err("`in` takes a list of at least one literal".into());
            }
            Ok(Condition::In { attr, values })
        }
        _ => Err(UNSUPPORTED.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(attr: &str, op: Op, operand: Operand) -> Condition {
        Condition::Compare {
            attr: attr.into(),
            op,
            operand,
        }
    }

    #[test]
    fn the_subset_parses() {
        let cases = [
            (
                "R.attr.authorId == P.id",
                cmp("authorId", Op::Eq, Operand::PrincipalId),
            ),
            (
                "P.id == R.attr.authorId",
                cmp("authorId", Op::Eq, Operand::PrincipalId),
            ),
            (
                "R.attr.archived == true",
                cmp("archived", Op::Eq, Operand::Literal(true.into())),
            ),
            (
                "R.attr.n != -3",
                cmp("n", Op::Ne, Operand::Literal(Value::Integer(-3))),
            ),
            (
                "R.attr.x == null",
                cmp("x", Op::Eq, Operand::Literal(Value::Null)),
            ),
            (
                "R.attr.t == 'a\\'b'",
                cmp("t", Op::Eq, Operand::Literal("a'b".into())),
            ),
            (
                "R.attr.team != P.attr.team",
                cmp("team", Op::Ne, Operand::PrincipalAttr("team".into())),
            ),
            (
                "R.attr.status in [\"draft\", 2, false]",
                Condition::In {
                    attr: "status".into(),
                    values: vec!["draft".into(), Value::Integer(2), false.into()],
                },
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(parse_expr(source), Ok(expected), "{source}");
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for source in [
            "R.attr.a",
            "R.attr.a == R.attr.b",
            "P.id == \"x\"",
            "R.attr.a > 1",
            "R.attr.a == 1.5",
            "R.attr.a == 1 && R.attr.b == 2",
            "size(R.attr.a) == 1",
            "R.id == P.id",
            "R.attr.a.b == 1",
            "R.attr.a in []",
            "R.attr.a in [P.id]",
            "\"x\" in [\"x\"]",
            "R.attr.a == \"x",
            "",
        ] {
            assert!(parse_expr(source).is_err(), "{source}");
        }
    }
}
