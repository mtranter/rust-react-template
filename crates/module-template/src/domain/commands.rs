use super::WidgetId;

#[derive(Debug, Clone)]
pub struct CreateWidget {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RenameWidget {
    pub widget_id: WidgetId,
    pub name: String,
}
