//! User input abstraction — decouples interactive prompts from dialoguer.
//!
//! Production uses [`TerminalInput`] (dialoguer). Tests inject [`ScriptedInput`]
//! (pre-programmed responses). E2E tests (separate process via assert_cmd) use
//! environment variables checked by TerminalInput before falling back to dialoguer.
//!
//! # E2E test environment variables
//!
//! - `NEX_TEST_PASSPHRASE` — bypass password prompts
//! - `NEX_TEST_CONFIRM` — bypass confirm prompts ("y"/"true" = yes, anything else = no)
//! - `NEX_TEST_INPUT` — bypass text input prompts

use anyhow::{Context, Result};

/// Abstract input provider for user interaction.
pub trait InputProvider: Send + Sync {
    /// Read a password (no echo).
    fn password(&self, prompt: &str) -> Result<String>;

    /// Read a password with confirmation (no echo, entered twice).
    fn password_with_confirm(&self, prompt: &str) -> Result<String>;

    /// Ask a yes/no question.
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool>;

    /// Read a text input with optional default.
    fn input_text(&self, prompt: &str, default: Option<&str>) -> Result<String>;

    /// Select from a list of items.
    fn select(&self, prompt: &str, items: &[String], default: usize) -> Result<usize>;
}

/// Production input — reads from terminal via dialoguer.
/// Falls back to environment variables for e2e test support.
pub struct TerminalInput;

impl InputProvider for TerminalInput {
    fn password(&self, prompt: &str) -> Result<String> {
        if let Ok(pp) = std::env::var("NEX_TEST_PASSPHRASE") {
            return Ok(pp);
        }
        dialoguer::Password::new()
            .with_prompt(prompt)
            .interact()
            .context("failed to read password")
    }

    fn password_with_confirm(&self, prompt: &str) -> Result<String> {
        if let Ok(pp) = std::env::var("NEX_TEST_PASSPHRASE") {
            return Ok(pp);
        }
        dialoguer::Password::new()
            .with_prompt(prompt)
            .with_confirmation("Confirm", "Values do not match")
            .interact()
            .context("failed to read password")
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
        if let Ok(val) = std::env::var("NEX_TEST_CONFIRM") {
            return Ok(val == "y" || val == "yes" || val == "true");
        }
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            return Ok(default);
        }
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .context("failed to read confirmation")
    }

    fn input_text(&self, prompt: &str, default: Option<&str>) -> Result<String> {
        if let Ok(val) = std::env::var("NEX_TEST_INPUT") {
            return Ok(val);
        }
        let mut builder = dialoguer::Input::<String>::new().with_prompt(prompt);
        if let Some(d) = default {
            builder = builder.default(d.to_string());
        }
        builder.interact_text().context("failed to read input")
    }

    fn select(&self, prompt: &str, items: &[String], default: usize) -> Result<usize> {
        if let Ok(val) = std::env::var("NEX_TEST_SELECT") {
            return Ok(val.parse().unwrap_or(default));
        }
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            return Ok(default);
        }
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()
            .context("failed to read selection")
    }
}

/// Test input — returns pre-programmed responses in order.
#[cfg(test)]
pub struct ScriptedInput {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
}

#[cfg(test)]
impl ScriptedInput {
    pub fn new(responses: Vec<&str>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().map(String::from).collect()),
        }
    }

    fn next_response(&self) -> Result<String> {
        self.responses
            .lock()
            .expect("scripted input lock")
            .pop_front()
            .context("ScriptedInput: no more responses queued")
    }
}

#[cfg(test)]
impl InputProvider for ScriptedInput {
    fn password(&self, _prompt: &str) -> Result<String> {
        self.next_response()
    }

    fn password_with_confirm(&self, _prompt: &str) -> Result<String> {
        self.next_response()
    }

    fn confirm(&self, _prompt: &str, default: bool) -> Result<bool> {
        match self.responses.lock().expect("lock").pop_front() {
            Some(r) => Ok(r == "y" || r == "yes" || r == "true"),
            None => Ok(default),
        }
    }

    fn input_text(&self, _prompt: &str, default: Option<&str>) -> Result<String> {
        match self.responses.lock().expect("lock").pop_front() {
            Some(r) if !r.is_empty() => Ok(r),
            _ => Ok(default.unwrap_or("").to_string()),
        }
    }

    fn select(&self, _prompt: &str, _items: &[String], default: usize) -> Result<usize> {
        match self.responses.lock().expect("lock").pop_front() {
            Some(r) => Ok(r.parse().unwrap_or(default)),
            None => Ok(default),
        }
    }
}

/// Global input provider. Defaults to TerminalInput.
static INPUT: std::sync::OnceLock<Box<dyn InputProvider>> = std::sync::OnceLock::new();

/// Get the active input provider.
pub fn input() -> &'static dyn InputProvider {
    INPUT.get_or_init(|| Box::new(TerminalInput)).as_ref()
}

/// Set a custom input provider (for unit tests). Must be called before first use.
#[cfg(test)]
pub fn set_input(provider: Box<dyn InputProvider>) {
    let _ = INPUT.set(provider);
}
