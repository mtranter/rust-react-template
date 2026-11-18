//! Keeps a list's SQL and `Filter::matches` from drifting apart (ADR 0040): the same plan must
//! hold the same rows whether a repository's query applies it or a service checks a loaded row.

use std::fmt::Debug;
use std::future::Future;

use crate::{Action, Authorizer, Principal, Resource, Rows};

/// For every principal and every action of `R` they aren't denied, the keys `list` returns for the
/// plan's rows against the keys of the `rows` its filter matches in memory. `rows` are what the
/// table holds; `list` queries them with the module's columns. Empty when they agree.
pub async fn disagreements<R, K, F, Fut>(
    authorizer: &Authorizer,
    principals: &[Principal],
    rows: &[R],
    key: impl Fn(&R) -> K,
    mut list: F,
) -> Vec<String>
where
    R: Resource,
    K: Ord + Debug,
    F: FnMut(Rows) -> Fut,
    Fut: Future<Output = Vec<K>>,
{
    let mut found = Vec::new();
    for principal in principals {
        for &action in R::Action::all() {
            let Some(scope) = authorizer.plan::<R>(principal, action).rows() else {
                continue;
            };
            let mut expected: Vec<K> = rows
                .iter()
                .filter(|row| scope.allows(*row))
                .map(&key)
                .collect();
            expected.sort();
            let mut listed = list(scope.clone()).await;
            listed.sort();
            if listed != expected {
                found.push(format!(
                    "{principal:?} {}s {scope:?}: the query held {listed:?}, the filter {expected:?}",
                    action.name()
                ));
            }
        }
    }
    found
}

/// [`disagreements`], failing the test on any.
pub async fn assert_lists_agree<R, K, F, Fut>(
    authorizer: &Authorizer,
    principals: &[Principal],
    rows: &[R],
    key: impl Fn(&R) -> K,
    list: F,
) where
    R: Resource,
    K: Ord + Debug,
    F: FnMut(Rows) -> Fut,
    Fut: Future<Output = Vec<K>>,
{
    let found = disagreements(authorizer, principals, rows, key, list).await;
    assert!(
        found.is_empty(),
        "the list's SQL and the filter disagree:\n{}",
        found.join("\n")
    );
}
