/// Known name mappings between package managers and binary names.
///
/// Each entry maps from a common name (binary name, brew name, shorthand)
/// to the canonical nixpkgs attribute. The resolve and duplicate-detection
/// paths both consult this table.
/// (input_name, nixpkgs_attr)
const ALIASES: &[(&str, &str)] = &[
    // ── CLI tools: binary name -> nixpkgs attr ─────────────────────────────
    ("rg", "ripgrep"),
    ("fd", "fd"),
    ("bat", "bat"),
    ("lg", "lazygit"),
    ("hx", "helix"),
    ("gh", "gh"),
    ("jq", "jq"),
    ("yq", "yq-go"),
    ("htop", "htop"),
    ("btop", "btop"),
    ("tmux", "tmux"),
    ("direnv", "direnv"),
    ("kubectl", "kubectl"),
    ("k9s", "k9s"),
    ("helm", "kubernetes-helm"),
    // ── Editors / IDEs ─────────────────────────────────────────────────────
    ("zed", "zed-editor"),
    ("neovim", "neovim"),
    ("nvim", "neovim"),
    ("code", "vscode"),
    ("visual-studio-code", "vscode"),
    ("sublime", "sublime4"),
    ("sublime-text", "sublime4"),
    // ── Communication ──────────────────────────────────────────────────────
    ("signal", "signal-desktop"),
    ("telegram", "telegram-desktop"),
    ("element", "element-desktop"),
    ("whatsapp", "whatsapp-for-linux"),
    ("slack", "slack"),
    ("discord", "discord"),
    ("thunderbird", "thunderbird"),
    ("zoom", "zoom-us"),
    // ── Browsers ───────────────────────────────────────────────────────────
    ("firefox", "firefox"),
    ("brave", "brave"),
    ("chrome", "google-chrome"),
    ("google-chrome", "google-chrome"),
    ("edge", "microsoft-edge"),
    // ── Media / creative ───────────────────────────────────────────────────
    ("vlc", "vlc"),
    ("mpv", "mpv"),
    ("obs", "obs-studio"),
    ("gimp", "gimp"),
    ("inkscape", "inkscape"),
    ("blender", "blender"),
    ("krita", "krita"),
    ("audacity", "audacity"),
    // ── Productivity ───────────────────────────────────────────────────────
    ("obsidian", "obsidian"),
    ("libreoffice", "libreoffice"),
    ("bitwarden", "bitwarden-desktop"),
    ("joplin", "joplin-desktop"),
    // ── Dev tools / runtimes ───────────────────────────────────────────────
    ("docker", "docker"),
    ("docker-desktop", "docker"),
    ("node", "nodejs"),
    ("nodejs", "nodejs"),
    ("terraform", "terraform"),
    ("hashicorp-terraform", "terraform"),
    ("vault", "vault"),
    ("hashicorp-vault", "vault"),
    // ── Identity ───────────────────────────────────────────────────────────
    ("1password", "_1password-gui"),
    ("1password-cli", "_1password"),
    // ── Python version aliases ─────────────────────────────────────────────
    ("python3", "python3"),
    ("python", "python3"),
];

/// Brew cask name mappings — (input_name, brew_cask_name).
/// Used by the resolver to check casks under the correct name when the
/// user types a shorthand or nixpkgs attr name.
const BREW_CASK_ALIASES: &[(&str, &str)] = &[
    // ── Editors ────────────────────────────────────────────────────────────
    ("vscode", "visual-studio-code"),
    ("code", "visual-studio-code"),
    ("zed", "zed"),
    ("zed-editor", "zed"),
    ("sublime", "sublime-text"),
    ("sublime4", "sublime-text"),
    // ── Communication ──────────────────────────────────────────────────────
    ("slack", "slack"),
    ("discord", "discord"),
    ("zoom", "zoom"),
    ("zoom-us", "zoom"),
    ("spotify", "spotify"),
    ("postman", "postman"),
    ("signal", "signal"),
    ("signal-desktop", "signal"),
    ("telegram", "telegram"),
    ("telegram-desktop", "telegram"),
    ("element", "element"),
    ("element-desktop", "element"),
    ("whatsapp", "whatsapp"),
    ("thunderbird", "thunderbird"),
    // ── Browsers ───────────────────────────────────────────────────────────
    ("firefox", "firefox"),
    ("chrome", "google-chrome"),
    ("google-chrome", "google-chrome"),
    ("brave", "brave-browser"),
    ("edge", "microsoft-edge"),
    ("microsoft-edge", "microsoft-edge"),
    // ── Media / creative ───────────────────────────────────────────────────
    ("vlc", "vlc"),
    ("obs", "obs"),
    ("obs-studio", "obs"),
    ("gimp", "gimp"),
    ("inkscape", "inkscape"),
    ("blender", "blender"),
    ("krita", "krita"),
    ("audacity", "audacity"),
    // ── Productivity ───────────────────────────────────────────────────────
    ("obsidian", "obsidian"),
    ("libreoffice", "libreoffice"),
    ("bitwarden", "bitwarden"),
    ("bitwarden-desktop", "bitwarden"),
    // ── Dev tools ──────────────────────────────────────────────────────────
    ("docker", "docker"),
    ("docker-desktop", "docker"),
    ("iterm2", "iterm2"),
    ("wezterm", "wezterm"),
    ("1password", "1password"),
    ("_1password-gui", "1password"),
    // ── Hashicorp ──────────────────────────────────────────────────────────
    ("terraform", "hashicorp-terraform"),
    ("vault", "hashicorp-vault"),
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
pub fn nixpkgs_attr(name: &str) -> &str {
    nixpkgs_attr_static(name).unwrap_or(name)
}

/// Look up the brew cask name for a given input.
/// Returns the cask name if known, or None.
pub fn brew_cask_name(name: &str) -> Option<&'static str> {
    for &(alias, cask) in BREW_CASK_ALIASES {
        if alias == name {
            return Some(cask);
        }
    }
    None
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

    #[test]
    fn test_common_app_aliases() {
        assert_eq!(nixpkgs_attr("signal"), "signal-desktop");
        assert_eq!(nixpkgs_attr("chrome"), "google-chrome");
        assert_eq!(nixpkgs_attr("node"), "nodejs");
        assert_eq!(nixpkgs_attr("yq"), "yq-go");
        assert_eq!(nixpkgs_attr("helm"), "kubernetes-helm");
        assert_eq!(nixpkgs_attr("obs"), "obs-studio");
    }

    #[test]
    fn test_brew_cask_common() {
        assert_eq!(brew_cask_name("signal"), Some("signal"));
        assert_eq!(brew_cask_name("signal-desktop"), Some("signal"));
        assert_eq!(brew_cask_name("edge"), Some("microsoft-edge"));
        assert_eq!(brew_cask_name("bitwarden-desktop"), Some("bitwarden"));
    }
}
