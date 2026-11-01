//! What `widgets` announces (spec §4): one topic per aggregate root, each event
//! carrying the root as it now is.

pub mod v1 {
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every widgets event carries: the widget's state but its audit
    /// fields. Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Widget {
        pub id: Uuid,
        pub name: String,
    }

    app_events::topic! {
        /// Everything that happens to a widget, keyed by its id.
        pub topic WidgetTopic("widgets.widget.v1") {
            source: "/widgets",
            entity: Widget,
            key: |widget| widget.id.to_string(),
            events: WidgetEvent, WidgetEventKind;
            WidgetCreated => Created: "widgets.widget_created",
            WidgetRenamed => Renamed: "widgets.widget_renamed",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [WidgetEvent::Created, WidgetEvent::Renamed];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&WidgetEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), WidgetEventKind::ALL.len());
        }
    }
}
