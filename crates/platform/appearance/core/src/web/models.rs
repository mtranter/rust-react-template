//! The version is an `i64` here, unlike other modules' strings: a Theme never saved is version
//! `0`, which the SPA compares as a number.

use crate::application::Versioned;
use crate::domain::Theme;
use poem_openapi::Object;
use std::collections::BTreeMap;

/// Each map is CSS variable names without their `--` (`primary`, `radius`) to values.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ThemeInput {
    /// Colour overrides for the light scheme.
    pub light: BTreeMap<String, String>,
    /// Colour overrides for the dark scheme.
    pub dark: BTreeMap<String, String>,
    /// Overrides both schemes share: radius, fonts, shadows.
    pub shared: BTreeMap<String, String>,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase", rename = "Theme")]
pub struct ThemeResponse {
    /// `0` until first saved; send it back in `If-Match` to change the Theme.
    pub version: i64,
    pub light: BTreeMap<String, String>,
    pub dark: BTreeMap<String, String>,
    pub shared: BTreeMap<String, String>,
    /// RFC 3339; `None` until first saved.
    pub updated_at: Option<String>,
    /// The subject who last saved it.
    pub updated_by: Option<String>,
}

impl From<Option<Versioned<Theme>>> for ThemeResponse {
    fn from(theme: Option<Versioned<Theme>>) -> Self {
        match theme {
            None => Self {
                version: 0,
                light: BTreeMap::new(),
                dark: BTreeMap::new(),
                shared: BTreeMap::new(),
                updated_at: None,
                updated_by: None,
            },
            Some(Versioned { entity, version }) => Self {
                version: version.as_i64(),
                light: entity.light().clone(),
                dark: entity.dark().clone(),
                shared: entity.shared().clone(),
                updated_at: Some(entity.updated_at().to_rfc3339()),
                updated_by: Some(entity.updated_by().to_owned()),
            },
        }
    }
}
