use anyhow::Result;
use console::style;

use crate::exec;

/// Where a package was found.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Nix,
    BrewCask,
    BrewFormula,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Nix => write!(f, "nixpkgs"),
            Source::BrewCask => write!(f, "brew cask"),
            Source::BrewFormula => write!(f, "brew formula"),
        }
    }
}

/// A package found in a particular source with its version.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub source: Source,
    pub version: String,
}

/// The resolution outcome.
pub enum Resolution {
    /// Only one source has it — use that.
    Single(Candidate),
    /// Multiple sources — nex recommends one but the user should confirm.
    Conflict {
        candidates: Vec<Candidate>,
        recommended: Source,
        reason: String,
    },
    /// Not found anywhere.
    NotFound,
}

/// Resolve a package across nixpkgs, brew casks, and brew formulae.
pub fn resolve(pkg: &str) -> Result<Resolution> {
    let mut candidates = Vec::new();

    // Check nixpkgs
    if let Some(version) = exec::nix_eval_version(pkg)? {
        candidates.push(Candidate {
            source: Source::Nix,
            version,
        });
    }

    // Check brew cask
    if let Some(version) = exec::brew_cask_info(pkg)? {
        candidates.push(Candidate {
            source: Source::BrewCask,
            version,
        });
    }

    // Check brew formula (only if not found as cask — formulae and casks rarely overlap)
    if !candidates.iter().any(|c| c.source == Source::BrewCask) {
        if let Some(version) = exec::brew_formula_info(pkg)? {
            candidates.push(Candidate {
                source: Source::BrewFormula,
                version,
            });
        }
    }

    match candidates.len() {
        0 => Ok(Resolution::NotFound),
        1 => {
            // Safe: len checked above
            #[allow(clippy::unwrap_used)]
            let candidate = candidates.into_iter().next().unwrap();
            Ok(Resolution::Single(candidate))
        }
        _ => {
            let (recommended, reason) = recommend(&candidates);
            Ok(Resolution::Conflict {
                candidates,
                recommended,
                reason,
            })
        }
    }
}

/// Decide which source to recommend when there's a conflict.
fn recommend(candidates: &[Candidate]) -> (Source, String) {
    let has_cask = candidates.iter().any(|c| c.source == Source::BrewCask);
    let has_nix = candidates.iter().any(|c| c.source == Source::Nix);

    if has_cask && has_nix {
        let nix_ver = candidates
            .iter()
            .find(|c| c.source == Source::Nix)
            .map(|c| c.version.as_str())
            .unwrap_or("");
        let cask_ver = candidates
            .iter()
            .find(|c| c.source == Source::BrewCask)
            .map(|c| c.version.as_str())
            .unwrap_or("");

        // If it exists as a cask, it's a GUI app — cask is almost always better on macOS
        // (proper .app bundle, code signing, Spotlight, auto-update)
        if nix_ver == cask_ver {
            return (
                Source::BrewCask,
                "GUI app — cask provides native .app bundle with code signing".into(),
            );
        }
        return (
            Source::BrewCask,
            format!(
                "GUI app — cask is {} (nix has {}), with native .app bundle",
                cask_ver, nix_ver
            ),
        );
    }

    // Default: prefer nix for CLI tools
    (
        Source::Nix,
        "CLI tool — nix provides declarative management".into(),
    )
}

/// Display the resolution to the user and return the chosen source.
/// Returns None if the user cancels.
pub fn prompt_resolution(
    pkg: &str,
    candidates: &[Candidate],
    recommended: &Source,
    reason: &str,
) -> Result<Option<Source>> {
    eprintln!();
    eprintln!("  {} found in multiple sources:", style(pkg).bold());
    eprintln!();

    for c in candidates {
        let marker = if c.source == *recommended {
            style("*").green().bold().to_string()
        } else {
            " ".to_string()
        };
        let source_label = format!("{:<14}", c.source.to_string());
        eprintln!(
            "  {}  {}  {}",
            marker,
            style(source_label).cyan(),
            c.version
        );
    }

    eprintln!();
    eprintln!("  {} {}", style("recommended:").dim(), reason,);
    eprintln!();

    // Build selection items
    let items: Vec<String> = candidates
        .iter()
        .map(|c| {
            let rec = if c.source == *recommended {
                " (recommended)"
            } else {
                ""
            };
            format!("{} {}{}", c.source, c.version, rec)
        })
        .collect();

    let default_idx = candidates
        .iter()
        .position(|c| c.source == *recommended)
        .unwrap_or(0);

    let selection = dialoguer::Select::new()
        .with_prompt("Install as")
        .items(&items)
        .default(default_idx)
        .interact_opt()?;

    match selection {
        Some(idx) => Ok(Some(candidates[idx].source.clone())),
        None => Ok(None),
    }
}
