//! What `appearance` announces (ADR 0012): the org's Theme as it now is.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// The org-wide Theme's overrides, by CSS variable name without its `--`. Only gains
    /// fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Theme {
        pub light: BTreeMap<String, String>,
        pub dark: BTreeMap<String, String>,
        pub shared: BTreeMap<String, String>,
        pub updated_at: DateTime<Utc>,
        /// The subject who saved it.
        pub updated_by: String,
    }

    app_events::topic! {
        /// Every change to the org's one Theme, keyed `org`.
        pub topic ThemeTopic("appearance.theme.v1") {
            source: "/appearance",
            retention: app_events::TimeDelta::days(30),
            entity: Theme,
            key: |_theme| "org".to_owned(),
            events: ThemeEvent, ThemeEventKind;
            ThemeUpdated => Updated: "appearance.theme_updated",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [ThemeEvent::Updated];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&ThemeEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), ThemeEventKind::ALL.len());
        }
    }
}
