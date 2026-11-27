//! What `ui-settings` announces (ADR 0012): the org's Theme and which apps it hides, each as it
//! now is.

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
        pub topic ThemeTopic("ui_settings.theme.v1") {
            source: "/ui-settings",
            retention: app_events::TimeDelta::days(30),
            entity: Theme,
            key: |_theme| "org".to_owned(),
            events: ThemeEvent, ThemeEventKind;
            ThemeUpdated => Updated: "ui_settings.theme_updated",
        }
    }

    /// The ids of the apps the org hides from its SPA (ADR 0052). Only gains fields, each
    /// `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AppVisibility {
        pub hidden: Vec<String>,
        pub updated_at: DateTime<Utc>,
        /// The subject who saved it.
        pub updated_by: String,
    }

    app_events::topic! {
        /// Every change to which apps the org hides, keyed `org`.
        pub topic AppVisibilityTopic("ui_settings.app_visibility.v1") {
            source: "/ui-settings",
            retention: app_events::TimeDelta::days(30),
            entity: AppVisibility,
            key: |_visibility| "org".to_owned(),
            events: AppVisibilityEvent, AppVisibilityEventKind;
            AppVisibilityUpdated => Updated: "ui_settings.app_visibility_updated",
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
            let events = [AppVisibilityEvent::Updated];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(
                    &AppVisibilityEvent::decode(event_type, context).unwrap(),
                    event
                );
            }
            assert_eq!(events.len(), AppVisibilityEventKind::ALL.len());
        }
    }
}
