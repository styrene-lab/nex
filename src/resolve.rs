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
        /// True when nix and brew have the same version (eligible for auto-preference).
        versions_equal: bool,
    },
    /// Not found anywhere.
    NotFound,
}

/// Full result of resolve(), including whether all sources were checked.
pub struct ResolveResult {
    pub resolution: Resolution,
    /// True if brew was available and checked during resolution.
    pub brew_checked: bool,
}

/// Resolve a package across nixpkgs, brew casks, and brew formulae.
pub fn resolve(pkg: &str) -> Result<ResolveResult> {
    let mut candidates = Vec::new();
    let brew_checked = exec::brew_available();

    // Check nixpkgs — try the canonical alias first, then the raw name
    let nix_attr = crate::aliases::nixpkgs_attr(pkg);
    let nix_version = exec::nix_eval_version(nix_attr)?.or(if nix_attr != pkg {
        exec::nix_eval_version(pkg)?
    } else {
        None
    });
    if let Some(version) = nix_version {
        candidates.push(Candidate {
            source: Source::Nix,
            version,
        });
    }

    if brew_checked {
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
    }

    let resolution = match candidates.len() {
        0 => Resolution::NotFound,
        1 => {
            // Safe: len checked above
            #[allow(clippy::unwrap_used)]
            let candidate = candidates.into_iter().next().unwrap();
            Resolution::Single(candidate)
        }
        _ => {
            let (recommended, reason, versions_equal) = recommend(&candidates);
            Resolution::Conflict {
                candidates,
                recommended,
                reason,
                versions_equal,
            }
        }
    };

    Ok(ResolveResult {
        resolution,
        brew_checked,
    })
}

/// Decide which source to recommend when there's a conflict.
/// Returns (recommended_source, reason, versions_equal).
fn recommend(candidates: &[Candidate]) -> (Source, String, bool) {
    let nix = candidates.iter().find(|c| c.source == Source::Nix);
    let brew = candidates
        .iter()
        .find(|c| c.source == Source::BrewCask || c.source == Source::BrewFormula);

    if let (Some(n), Some(b)) = (nix, brew) {
        let eq = n.version == b.version;

        if eq {
            // Same version — recommend nix (declarative, reproducible)
            return (
                Source::Nix,
                "same version in both — nix provides declarative management".into(),
                true,
            );
        }

        // Brew has a different (likely newer) version — recommend brew
        let brew_label = if b.source == Source::BrewCask {
            "cask"
        } else {
            "formula"
        };
        return (
            b.source.clone(),
            format!("brew {brew_label} is {} (nix has {})", b.version, n.version),
            false,
        );
    }

    // Fallback: prefer nix
    (
        Source::Nix,
        "nix provides declarative management".into(),
        false,
    )
}

/// Result of an interactive resolution prompt.
pub struct PromptResult {
    pub source: Source,
    /// If true, the user wants to always pick nix for equal-version conflicts.
    pub remember_nix: bool,
}

/// Display the resolution to the user and return the chosen source.
/// Returns None if the user cancels.
/// When `versions_equal` is true, offers an "always use nix" option.
pub fn prompt_resolution(
    pkg: &str,
    candidates: &[Candidate],
    recommended: &Source,
    reason: &str,
    versions_equal: bool,
) -> Result<Option<PromptResult>> {
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
    eprintln!("  {} {}", style("recommended:").dim(), reason);
    eprintln!();

    // Build selection items
    let mut items: Vec<String> = candidates
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

    // When versions match, offer an "always nix" option
    if versions_equal {
        items.push("Always use nix when versions match".into());
    }

    let default_idx = candidates
        .iter()
        .position(|c| c.source == *recommended)
        .unwrap_or(0);

    // When not interactive, use the recommended default
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Ok(Some(PromptResult {
            source: recommended.clone(),
            remember_nix: false,
        }));
    }

    let selection = dialoguer::Select::new()
        .with_prompt("Install as")
        .items(&items)
        .default(default_idx)
        .interact_opt()?;

    match selection {
        Some(idx) if idx < candidates.len() => Ok(Some(PromptResult {
            source: candidates[idx].source.clone(),
            remember_nix: false,
        })),
        Some(_) => {
            // "Always use nix" option selected
            Ok(Some(PromptResult {
                source: Source::Nix,
                remember_nix: true,
            }))
        }
        None => Ok(None),
    }
}
