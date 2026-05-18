//! Minimal menu model shared by CLI prompts and richer TUIs.
//!
//! This is intentionally a supporting structure, not a UI framework. Callers
//! build small serializable menus; frontends decide how to render them.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::input::{input, InputProvider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Menu {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_item_id: Option<String>,
    #[serde(default)]
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            default_item_id: None,
            items: Vec::new(),
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn default_item(mut self, item_id: impl Into<String>) -> Self {
        self.default_item_id = Some(item_id.into());
        self
    }

    pub fn item(mut self, item: MenuItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn selectable_items(&self) -> Vec<(usize, &MenuItem)> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.disabled)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            disabled: false,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MenuSelection {
    pub menu_id: String,
    pub item_id: String,
    pub item_index: usize,
}

pub trait MenuPresenter {
    fn select(&self, menu: &Menu) -> Result<MenuSelection>;
}

pub struct InputMenuPresenter<'a> {
    input: &'a dyn InputProvider,
}

impl<'a> InputMenuPresenter<'a> {
    pub fn new(input: &'a dyn InputProvider) -> Self {
        Self { input }
    }
}

impl Default for InputMenuPresenter<'static> {
    fn default() -> Self {
        Self { input: input() }
    }
}

impl MenuPresenter for InputMenuPresenter<'_> {
    fn select(&self, menu: &Menu) -> Result<MenuSelection> {
        let selectable = menu.selectable_items();
        if selectable.is_empty() {
            bail!("menu {} has no selectable items", menu.id);
        }

        let labels: Vec<String> = selectable
            .iter()
            .map(|(_, item)| render_label(item))
            .collect();
        let default = default_selectable_index(menu, &selectable);
        let selected = self.input.select(&menu.title, &labels, default)?;
        let (item_index, item) = selectable
            .get(selected)
            .copied()
            .unwrap_or_else(|| selectable[default]);

        Ok(MenuSelection {
            menu_id: menu.id.clone(),
            item_id: item.id.clone(),
            item_index,
        })
    }
}

pub fn select(menu: &Menu) -> Result<MenuSelection> {
    InputMenuPresenter::default().select(menu)
}

fn render_label(item: &MenuItem) -> String {
    match &item.description {
        Some(description) if !description.is_empty() => {
            format!("{} - {}", item.label, description)
        }
        _ => item.label.clone(),
    }
}

fn default_selectable_index(menu: &Menu, selectable: &[(usize, &MenuItem)]) -> usize {
    let Some(default_id) = menu.default_item_id.as_deref() else {
        return 0;
    };

    selectable
        .iter()
        .position(|(_, item)| item.id == default_id)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    struct FixedSelect(usize);

    impl InputProvider for FixedSelect {
        fn password(&self, _prompt: &str) -> Result<String> {
            Ok(String::new())
        }

        fn password_with_confirm(&self, _prompt: &str) -> Result<String> {
            Ok(String::new())
        }

        fn confirm(&self, _prompt: &str, default: bool) -> Result<bool> {
            Ok(default)
        }

        fn input_text(&self, _prompt: &str, default: Option<&str>) -> Result<String> {
            Ok(default.unwrap_or_default().to_string())
        }

        fn select(&self, _prompt: &str, _items: &[String], _default: usize) -> Result<usize> {
            Ok(self.0)
        }
    }

    #[test]
    fn selects_enabled_item_by_rendered_index() -> Result<()> {
        let menu = Menu::new("forge-target", "Forge target")
            .item(MenuItem::new("bundle", "Bundle only"))
            .item(MenuItem::new("usb", "USB installer"));

        let selection = InputMenuPresenter::new(&FixedSelect(1)).select(&menu)?;

        assert_eq!(selection.menu_id, "forge-target");
        assert_eq!(selection.item_id, "usb");
        assert_eq!(selection.item_index, 1);
        Ok(())
    }

    #[test]
    fn skips_disabled_items() -> Result<()> {
        let menu = Menu::new("forge-target", "Forge target")
            .item(MenuItem::new("bundle", "Bundle only").disabled(true))
            .item(MenuItem::new("usb", "USB installer"));

        let selection = InputMenuPresenter::new(&FixedSelect(0)).select(&menu)?;

        assert_eq!(selection.item_id, "usb");
        assert_eq!(selection.item_index, 1);
        Ok(())
    }

    #[test]
    fn serializes_as_plain_data() -> Result<()> {
        let menu = Menu::new("forge-target", "Forge target")
            .description("Select the artifact to produce")
            .default_item("bundle")
            .item(MenuItem::new("bundle", "Bundle only"));

        let encoded = serde_json::to_string(&menu).context("serialize menu")?;
        let decoded: Menu = serde_json::from_str(&encoded).context("decode menu")?;

        assert_eq!(decoded, menu);
        Ok(())
    }
}
