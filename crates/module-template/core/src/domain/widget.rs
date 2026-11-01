use super::{CreateWidget, RenameWidget, WidgetError, WidgetId};
use chrono::{DateTime, Utc};

const MAX_NAME_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    id: WidgetId,
    name: String,
    created_at: DateTime<Utc>,
}

impl Widget {
    pub fn create(
        id: WidgetId,
        cmd: CreateWidget,
        now: DateTime<Utc>,
    ) -> Result<Self, WidgetError> {
        Ok(Self {
            id,
            name: name(&cmd.name)?,
            created_at: now,
        })
    }

    /// On error the widget is left unchanged.
    pub fn rename(&mut self, cmd: RenameWidget) -> Result<(), WidgetError> {
        self.name = name(&cmd.name)?;
        Ok(())
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(id: WidgetId, name: String, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            name,
            created_at,
        }
    }

    pub fn id(&self) -> WidgetId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

fn name(raw: &str) -> Result<String, WidgetError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(WidgetError::EmptyName);
    }
    if name.len() > MAX_NAME_LEN {
        return Err(WidgetError::NameTooLong { max: MAX_NAME_LEN });
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(name: &str) -> Result<Widget, WidgetError> {
        Widget::create(
            WidgetId::new(),
            CreateWidget { name: name.into() },
            Utc::now(),
        )
    }

    #[test]
    fn create_trims_and_bounds_the_name() {
        assert_eq!(create("  First ").unwrap().name(), "First");
        assert_eq!(create("   "), Err(WidgetError::EmptyName));
        assert_eq!(
            create(&"x".repeat(MAX_NAME_LEN + 1)),
            Err(WidgetError::NameTooLong { max: MAX_NAME_LEN })
        );
    }

    #[test]
    fn a_rejected_rename_leaves_the_widget_unchanged() {
        let mut widget = create("First").unwrap();
        let before = widget.clone();
        let result = widget.rename(RenameWidget {
            widget_id: widget.id(),
            name: "".into(),
        });
        assert_eq!(result, Err(WidgetError::EmptyName));
        assert_eq!(widget, before);
    }
}
