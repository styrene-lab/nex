/// Known name mappings between package managers and binary names.
///
/// Each entry maps from a common name (binary name, brew name, shorthand)
/// to the canonical nixpkgs attribute. The resolve and duplicate-detection
/// paths both consult this table.

/// (input_name, nixpkgs_attr)
const ALIASES: &[(&str, &str)] = &[
    // Binary name -> nixpkgs attr
    ("rg", "ripgrep"),
    ("fd", "fd"),
    ("bat", "bat"),
    ("lg", "lazygit"),
    ("hx", "helix"),
    // Brew name -> nixpkgs attr (where they differ)
    ("zed", "zed-editor"),
    ("wezterm", "wezterm"),
    ("neovim", "neovim"),
    ("nvim", "neovim"),
    ("code", "vscode"),
    ("visual-studio-code", "vscode"),
    ("iterm2", "iterm2"),
    ("1password", "_1password-gui"),
    ("1password-cli", "_1password"),
    ("docker", "docker"),
    ("docker-desktop", "docker"),
    // Python version aliases
    ("python3", "python3"),
    ("python", "python3"),
];

/// Look up the nixpkgs attribute for a given name.
/// Returns the canonical attr if found, or None.
pub fn nixpkgs_attr_static(name: &str) -> Option<&'static str> {
    for &(alias, attr) in ALIASES {
        if alias == name {
            return Some(attr);
        }
    }
    None
}

/// Look up the nixpkgs attribute for a given name.
/// Returns the canonical attr if found, or the original name.
pub fn nixpkgs_attr<'a>(name: &'a str) -> &'a str {
    nixpkgs_attr_static(name).unwrap_or(name)
}

/// Get all names that map to the same nixpkgs attribute as the given name.
/// Includes the canonical attr itself. Used for duplicate detection.
pub fn all_names_for(name: &str) -> Vec<&'static str> {
    let canonical = match nixpkgs_attr_static(name) {
        Some(c) => c,
        None => return Vec::new(), // no known aliases
    };
    let mut names: Vec<&'static str> = ALIASES
        .iter()
        .filter(|&&(_, attr)| attr == canonical)
        .map(|&(alias, _)| alias)
        .collect();
    if !names.contains(&canonical) {
        names.push(canonical);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_alias() {
        assert_eq!(nixpkgs_attr("rg"), "ripgrep");
        assert_eq!(nixpkgs_attr("zed"), "zed-editor");
        assert_eq!(nixpkgs_attr("code"), "vscode");
    }

    #[test]
    fn test_unknown_passthrough() {
        assert_eq!(nixpkgs_attr("htop"), "htop");
        assert_eq!(nixpkgs_attr("git"), "git");
    }

    #[test]
    fn test_all_names() {
        let names = all_names_for("rg");
        assert!(names.contains(&"rg"));
        assert!(names.contains(&"ripgrep"));
    }

    #[test]
    fn test_all_names_code() {
        let names = all_names_for("code");
        assert!(names.contains(&"code"));
        assert!(names.contains(&"visual-studio-code"));
        assert!(names.contains(&"vscode"));
    }
}
