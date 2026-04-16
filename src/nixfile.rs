/// Describes an editable list inside a nix file.
#[derive(Debug, Clone)]
pub struct NixList {
    /// Regex-free prefix of the opening line (trimmed for matching).
    pub open_line: &'static str,
    /// Regex-free prefix of the closing line.
    pub close_line: &'static str,
    /// Number of spaces before each item.
    pub item_indent: usize,
    /// Whether items are quoted strings (`"foo"`) or bare identifiers (`foo`).
    pub quoted: bool,
}

impl NixList {
    /// Format a package name as it would appear in the file (with indent + quoting).
    pub fn format_item(&self, pkg: &str) -> String {
        let indent = " ".repeat(self.item_indent);
        if self.quoted {
            format!("{indent}\"{pkg}\"")
        } else {
            format!("{indent}{pkg}")
        }
    }

    /// Extract the package name from a line, if it matches the expected format.
    pub fn parse_item(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            return None;
        }
        if self.quoted {
            // Match `"package-name"` possibly followed by inline comment
            let stripped = trimmed.trim_start_matches('"');
            let end = stripped.find('"')?;
            Some(stripped[..end].to_string())
        } else {
            // Bare identifier — take first word (ignore inline comments)
            let word = trimmed.split_whitespace().next()?;
            // Skip nix keywords / structural tokens
            if word.starts_with('[')
                || word.starts_with(']')
                || word.starts_with('{')
                || word.starts_with('}')
                || word.contains('=')
                || word.contains('.')
                || word.contains('/')
            {
                return None;
            }
            Some(word.to_string())
        }
    }
}

// Known list definitions matching the macos-nix repo structure.

pub const NIX_PACKAGES: NixList = NixList {
    open_line: "home.packages = with pkgs; [",
    close_line: "];",
    item_indent: 4,
    quoted: false,
};

pub const HOMEBREW_BREWS: NixList = NixList {
    open_line: "brews = [",
    close_line: "];",
    item_indent: 6,
    quoted: true,
};

pub const HOMEBREW_CASKS: NixList = NixList {
    open_line: "casks = [",
    close_line: "];",
    item_indent: 6,
    quoted: true,
};
