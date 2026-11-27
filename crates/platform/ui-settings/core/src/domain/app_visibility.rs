use super::{AppVisibilityError, AppVisibilityEvent};
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

const MAX_APPS: usize = 100;
const MAX_ID_LEN: usize = 64;

/// Which apps the org hides from its SPA (ADR 0052), by the id of each app's Capability. Hiding
/// is a convenience, not protection: the apps' operations still answer. Ids aren't checked
/// against the registered Capabilities, as the SPA's registry is what draws apps; an id it
/// doesn't know hides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppVisibility {
    hidden: BTreeSet<String>,
    updated_at: DateTime<Utc>,
    updated_by: String,
}

impl AppVisibility {
    pub fn save(
        hidden: Vec<String>,
        by: String,
        now: DateTime<Utc>,
    ) -> Result<(Self, AppVisibilityEvent), AppVisibilityError> {
        let mut faults = Vec::new();
        if hidden.len() > MAX_APPS {
            faults.push(("hidden".to_owned(), format!("at most {MAX_APPS} apps")));
        }
        for (i, id) in hidden.iter().enumerate() {
            let fault = if id.is_empty() || id.len() > MAX_ID_LEN {
                Some(format!("must be 1 to {MAX_ID_LEN} characters"))
            } else if !(id.starts_with(|c: char| c.is_ascii_lowercase())
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            {
                // A Capability's id, as `capability.yaml` allows it.
                Some(
                    "must be an app's id: lowercase letters, digits and '-', from a letter"
                        .to_owned(),
                )
            } else {
                None
            };
            if let Some(fault) = fault {
                faults.push((format!("hidden.{i}"), fault));
            }
        }
        if !faults.is_empty() {
            return Err(AppVisibilityError { faults });
        }
        let visibility = Self {
            hidden: hidden.into_iter().collect(),
            updated_at: now,
            updated_by: by,
        };
        Ok((visibility, AppVisibilityEvent::Updated))
    }

    pub fn restore(hidden: Vec<String>, updated_at: DateTime<Utc>, updated_by: String) -> Self {
        Self {
            hidden: hidden.into_iter().collect(),
            updated_at,
            updated_by,
        }
    }

    pub fn hidden(&self) -> &BTreeSet<String> {
        &self.hidden
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn updated_by(&self) -> &str {
        &self.updated_by
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(hidden: &[&str]) -> Result<AppVisibility, Vec<String>> {
        AppVisibility::save(
            hidden.iter().map(|s| s.to_string()).collect(),
            "a".into(),
            Utc::now(),
        )
        .map(|(v, _)| v)
        .map_err(|err| err.faults.into_iter().map(|(path, _)| path).collect())
    }

    #[test]
    fn ids_are_kept_once_in_order() {
        let saved = saved(&["shop", "crm", "shop"]).unwrap();
        assert_eq!(saved.hidden().iter().collect::<Vec<_>>(), ["crm", "shop"]);
    }

    #[test]
    fn ids_that_are_no_app_id_are_refused_where_they_are() {
        assert_eq!(
            saved(&["ok", "", "Not OK", &"a".repeat(65)]),
            Err(vec![
                "hidden.1".into(),
                "hidden.2".into(),
                "hidden.3".into()
            ])
        );
    }
}
