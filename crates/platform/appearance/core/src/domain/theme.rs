use super::{ThemeError, ThemeEvent};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// CSS variable names without their `--`, mapped to their values.
pub type Overrides = BTreeMap<String, String>;

/// The variables a colour scheme (`light`, `dark`) may override: the SPA's design tokens.
const COLOURS: &[&str] = &[
    "background",
    "foreground",
    "card",
    "card-foreground",
    "popover",
    "popover-foreground",
    "primary",
    "primary-foreground",
    "secondary",
    "secondary-foreground",
    "muted",
    "muted-foreground",
    "accent",
    "accent-foreground",
    "destructive",
    "border",
    "input",
    "ring",
    "chart-1",
    "chart-2",
    "chart-3",
    "chart-4",
    "chart-5",
    "sidebar",
    "sidebar-foreground",
    "sidebar-primary",
    "sidebar-primary-foreground",
    "sidebar-accent",
    "sidebar-accent-foreground",
    "sidebar-border",
    "sidebar-ring",
    "highlight",
    "highlight-foreground",
    "success",
    "success-foreground",
    "warning",
    "warning-foreground",
    "info",
    "info-foreground",
];

/// The variables both schemes share.
const SHARED: &[&str] = &[
    "radius",
    "font-sans",
    "font-heading",
    "font-mono",
    "shadow-2xs",
    "shadow-xs",
    "shadow-sm",
    "shadow-md",
    "shadow-lg",
    "shadow-xl",
    "shadow-2xl",
];

const MAX_VALUE_LEN: usize = 200;

/// Values are injected into a stylesheet, so nothing that could end a declaration or rule,
/// open markup, escape, or fetch.
const FORBIDDEN: &[&str] = &[";", "{", "}", "<", ">", "\\", "url("];

/// The org's one Theme (ADR 0051): overrides of the SPA's CSS variables, applied at runtime.
/// Empty maps are the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    light: Overrides,
    dark: Overrides,
    shared: Overrides,
    updated_at: DateTime<Utc>,
    updated_by: String,
}

impl Theme {
    /// The Theme as `by` saves it at `now`; empty maps reset it to the defaults.
    pub fn save(
        light: Overrides,
        dark: Overrides,
        shared: Overrides,
        by: String,
        now: DateTime<Utc>,
    ) -> Result<(Self, ThemeEvent), ThemeError> {
        let mut faults = Vec::new();
        check("light", &light, COLOURS, &mut faults);
        check("dark", &dark, COLOURS, &mut faults);
        check("shared", &shared, SHARED, &mut faults);
        if !faults.is_empty() {
            return Err(ThemeError { faults });
        }
        let trimmed = |m: Overrides| {
            m.into_iter()
                .map(|(k, v)| (k, v.trim().to_owned()))
                .collect()
        };
        let theme = Self {
            light: trimmed(light),
            dark: trimmed(dark),
            shared: trimmed(shared),
            updated_at: now,
            updated_by: by,
        };
        Ok((theme, ThemeEvent::Updated))
    }

    pub fn restore(
        light: Overrides,
        dark: Overrides,
        shared: Overrides,
        updated_at: DateTime<Utc>,
        updated_by: String,
    ) -> Self {
        Self {
            light,
            dark,
            shared,
            updated_at,
            updated_by,
        }
    }

    pub fn light(&self) -> &Overrides {
        &self.light
    }

    pub fn dark(&self) -> &Overrides {
        &self.dark
    }

    pub fn shared(&self) -> &Overrides {
        &self.shared
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn updated_by(&self) -> &str {
        &self.updated_by
    }
}

fn check(map: &str, overrides: &Overrides, allowed: &[&str], faults: &mut Vec<(String, String)>) {
    for (name, value) in overrides {
        let path = format!("{map}.{name}");
        let value = value.trim();
        let fault = if !allowed.contains(&name.as_str()) {
            Some(format!("`{name}` is not a variable {map} may override"))
        } else if value.is_empty() {
            Some("must not be empty".to_owned())
        } else if value.chars().count() > MAX_VALUE_LEN {
            Some(format!("must be at most {MAX_VALUE_LEN} characters"))
        } else {
            FORBIDDEN
                .iter()
                .find(|bad| value.to_ascii_lowercase().contains(*bad))
                .map(|bad| format!("must not contain `{bad}`"))
        };
        if let Some(fault) = fault {
            faults.push((path, fault));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(light: &[(&str, &str)], shared: &[(&str, &str)]) -> Result<Theme, Vec<String>> {
        let map = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Theme::save(
            map(light),
            Overrides::new(),
            map(shared),
            "a".into(),
            Utc::now(),
        )
        .map(|(theme, _)| theme)
        .map_err(|err| err.faults.into_iter().map(|(path, _)| path).collect())
    }

    #[test]
    fn allowed_overrides_are_kept_trimmed() {
        let theme = saved(
            &[("primary", " oklch(0.5 0.2 250) ")],
            &[("radius", "0.5rem")],
        )
        .unwrap();
        assert_eq!(theme.light()["primary"], "oklch(0.5 0.2 250)");
        assert_eq!(theme.shared()["radius"], "0.5rem");
    }

    #[test]
    fn unknown_names_are_refused_where_they_are() {
        assert_eq!(
            saved(&[("radius", "1rem")], &[("primary", "red")]),
            Err(vec!["light.radius".into(), "shared.primary".into()])
        );
    }

    #[test]
    fn values_that_could_escape_the_stylesheet_are_refused() {
        for bad in [
            "red; x: y",
            "a{",
            "}",
            "<b>",
            "a\\0",
            "URL(x)",
            "",
            "  ",
            &"a".repeat(201),
        ] {
            assert_eq!(
                saved(&[("primary", bad)], &[]),
                Err(vec!["light.primary".into()]),
                "{bad:?}"
            );
        }
    }
}
