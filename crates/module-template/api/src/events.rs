//! What `widgets` announces (spec §4): one event per state change, each carrying
//! the widget as it now is. Stored as JSON, so a version only gains fields, each
//! `#[serde(default)]`.

pub mod v1 {
    use app_events::{Event, NoContext};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    const SOURCE: &str = "/widgets";

    /// What every widgets event carries: the widget's state but its audit fields.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Widget {
        pub id: Uuid,
        pub name: String,
    }

    pub struct WidgetCreated;

    impl Event for WidgetCreated {
        const TYPE: &'static str = "widgets.widget_created.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = Widget;
        type Context = NoContext;

        fn subject(widget: &Widget) -> String {
            widget.id.to_string()
        }
    }

    pub struct WidgetRenamed;

    impl Event for WidgetRenamed {
        const TYPE: &'static str = "widgets.widget_renamed.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = Widget;
        type Context = NoContext;

        fn subject(widget: &Widget) -> String {
            widget.id.to_string()
        }
    }
}
