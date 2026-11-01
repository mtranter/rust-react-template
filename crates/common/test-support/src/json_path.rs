//! ```
//! use app_test_support::{assert_json_exists, assert_json_matches, assert_json_not_exists};
//! use serde_json::json;
//!
//! let note = json!({ "id": "generated", "title": "Third", "tags": ["a", "b"] });
//! assert_json_exists!(note, "$.id"); // unpredictable, but must be present
//! assert_json_matches!(note, "$.title", "Third");
//! assert_json_matches!(note, "$.tags", vec!["a", "b"]);
//! assert_json_not_exists!(note, "$.password");
//! ```
//!
//! Matching is strict (no extra keys, exact array order) and compares against
//! the **first** value a path selects.

use assert_json_diff::{CompareMode, Config, assert_json_matches_no_panic};
use jsonpath_rust::JsonPath;
use serde_json::Value;

#[derive(Debug)]
pub struct PathValue(Value);

impl From<&str> for PathValue {
    fn from(s: &str) -> Self {
        PathValue(Value::String(s.to_string()))
    }
}

impl From<String> for PathValue {
    fn from(s: String) -> Self {
        PathValue(Value::String(s))
    }
}

impl From<f64> for PathValue {
    fn from(n: f64) -> Self {
        PathValue(Value::Number(serde_json::Number::from_f64(n).unwrap()))
    }
}

impl From<i64> for PathValue {
    fn from(n: i64) -> Self {
        PathValue(Value::Number(n.into()))
    }
}

impl From<bool> for PathValue {
    fn from(b: bool) -> Self {
        PathValue(Value::Bool(b))
    }
}

impl From<Value> for PathValue {
    fn from(v: Value) -> Self {
        PathValue(v)
    }
}

impl From<()> for PathValue {
    fn from(_: ()) -> Self {
        PathValue(Value::Null)
    }
}

impl<T> From<Vec<T>> for PathValue
where
    T: Into<PathValue>,
{
    fn from(v: Vec<T>) -> Self {
        PathValue(Value::Array(
            v.into_iter().map(|item| item.into().0).collect(),
        ))
    }
}

pub trait JsonPathAssert {
    fn assert_path_matches<T: Into<PathValue>>(&self, path: &str, expected: T);
    fn assert_path_exists(&self, path: &str);
    fn assert_path_not_exists(&self, path: &str);
    /// Panics if `path` is not valid JSONPath.
    fn get_path_value(&self, path: &str) -> Option<Value>;
}

impl JsonPathAssert for Value {
    fn assert_path_matches<T: Into<PathValue>>(&self, path: &str, expected: T) {
        let config = Config::new(CompareMode::Strict);
        let found = self
            .query(path)
            .unwrap_or_else(|e| panic!("Invalid JSON path '{}': {}", path, e));

        assert!(!found.is_empty(), "No value found at path '{}'", path);

        let actual = &found[0];
        let expected = expected.into().0;

        if let Err(e) = assert_json_matches_no_panic(actual, &expected, config) {
            panic!("Assertion failed for path '{}': {}", path, e);
        }
    }

    fn assert_path_exists(&self, path: &str) {
        let found = self
            .query(path)
            .unwrap_or_else(|e| panic!("Invalid JSON path '{}': {}", path, e));

        assert!(
            !found.is_empty(),
            "Expected value to exist at path '{}' but found none",
            path
        );
    }

    fn assert_path_not_exists(&self, path: &str) {
        let found = self
            .query(path)
            .unwrap_or_else(|e| panic!("Invalid JSON path '{}': {}", path, e));

        assert!(
            found.is_empty(),
            "Expected no value at path '{}' but found {:?}",
            path,
            found[0]
        );
    }

    fn get_path_value(&self, path: &str) -> Option<Value> {
        let found = self
            .query(path)
            .unwrap_or_else(|e| panic!("Invalid JSON path '{}': {}", path, e));

        found.into_iter().next().cloned()
    }
}

// The macros call through a fully-qualified `Value` receiver, so callers need no
// trait import and anything that derefs to `Value` (a driver's view) works.

#[macro_export]
macro_rules! assert_json_matches {
    ($value:expr, $path:expr, $expected:expr) => {
        <$crate::__private::Value as $crate::JsonPathAssert>::assert_path_matches(
            &$value, $path, $expected,
        )
    };
}

#[macro_export]
macro_rules! assert_json_exists {
    ($value:expr, $path:expr) => {
        <$crate::__private::Value as $crate::JsonPathAssert>::assert_path_exists(&$value, $path)
    };
}

#[macro_export]
macro_rules! assert_json_not_exists {
    ($value:expr, $path:expr) => {
        <$crate::__private::Value as $crate::JsonPathAssert>::assert_path_not_exists(&$value, $path)
    };
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_assert_matches_string() {
        let value = json!({
            "name": "John",
            "age": 30,
            "address": {
                "street": "123 Main St"
            }
        });

        assert_json_matches!(value, "$.name", "John");
        assert_json_matches!(value, "$.address.street", "123 Main St");
    }

    #[test]
    #[should_panic(expected = "Assertion failed for path '$.name'")]
    fn test_assert_matches_string_failure() {
        let value = json!({
            "name": "John"
        });

        assert_json_matches!(value, "$.name", "Jane");
    }

    #[test]
    fn test_assert_matches_number() {
        let value = json!({
            "age": 30,
            "score": 95.5
        });

        assert_json_matches!(value, "$.age", 30);
        assert_json_matches!(value, "$.score", 95.5);
    }

    #[test]
    #[should_panic(expected = "Assertion failed for path '$.age'")]
    fn test_assert_matches_number_failure() {
        let value = json!({
            "age": 30
        });

        assert_json_matches!(value, "$.age", 31);
    }

    #[test]
    fn test_assert_matches_array() {
        let value = json!({
            "tags": ["rust", "testing"]
        });

        assert_json_matches!(value, "$.tags", vec!["rust", "testing"]);
    }

    #[test]
    #[should_panic(expected = "Assertion failed for path '$.tags'")]
    fn test_assert_matches_array_failure() {
        let value = json!({
            "tags": ["rust", "testing"]
        });

        assert_json_matches!(value, "$.tags", vec!["rust"]);
    }

    #[test]
    fn test_assert_exists() {
        let value = json!({
            "name": "John",
            "age": null
        });

        assert_json_exists!(value, "$.name");
        assert_json_exists!(value, "$.age");
    }

    #[test]
    #[should_panic(expected = "Expected value to exist at path '$.address' but found none")]
    fn test_assert_exists_failure() {
        let value = json!({
            "name": "John"
        });

        assert_json_exists!(value, "$.address");
    }

    #[test]
    fn test_assert_not_exists() {
        let value = json!({
            "name": "John"
        });

        assert_json_not_exists!(value, "$.address");
    }

    #[test]
    #[should_panic(expected = "Expected no value at path '$.name'")]
    fn test_assert_not_exists_failure() {
        let value = json!({
            "name": "John"
        });

        assert_json_not_exists!(value, "$.name");
    }

    #[test]
    fn test_complex_json_path() {
        let value = json!({
            "store": {
                "books": [
                    {
                        "title": "Book 1",
                        "price": 10.0
                    },
                    {
                        "title": "Book 2",
                        "price": 20.0
                    }
                ]
            }
        });

        assert_json_matches!(value, "$.store.books[0].title", "Book 1");
        assert_json_matches!(value, "$.store.books[1].price", 20.0);
    }
}
