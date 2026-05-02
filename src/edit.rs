use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use crate::nixfile::NixList;

/// Find the line range (start inclusive, end inclusive) of a NixList in the file lines.
fn find_list_range(lines: &[String], list: &NixList) -> Result<(usize, usize)> {
    tracing::trace!(open_line = list.open_line, "searching for list range");
    let open = lines
        .iter()
        .position(|l| l.trim_start().starts_with(list.open_line))
        .context(format!("could not find list opening: {}", list.open_line))?;

    // Walk forward from open to find the matching close.
    // The close must be at the same or lesser indentation as the open line.
    let open_indent = lines[open].len() - lines[open].trim_start().len();
    let close = lines
        .iter()
        .enumerate()
        .skip(open + 1)
        .find(|(_, l)| {
            let trimmed = l.trim_start();
            let indent = l.len() - trimmed.len();
            trimmed.starts_with(list.close_line) && indent <= open_indent
        })
        .map(|(i, _)| i)
        .context(format!(
            "could not find list closing for: {}",
            list.open_line
        ))?;

    Ok((open, close))
}

/// Check whether a package is present in a list within the given file.
pub fn contains(path: &Path, list: &NixList, pkg: &str) -> Result<bool> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<String> = content.lines().map(String::from).collect();

    let (open, close) = match find_list_range(&lines, list) {
        Ok(range) => range,
        Err(_) => return Ok(false),
    };

    for line in &lines[open + 1..close] {
        if let Some(name) = list.parse_item(line) {
            if name == pkg {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Validate that a package name is safe to insert into a nix file.
/// Rejects names with characters that could break nix syntax or enable injection.
fn validate_pkg_name(pkg: &str) -> Result<()> {
    if pkg.is_empty() {
        anyhow::bail!("package name cannot be empty");
    }
    for ch in pkg.chars() {
        if !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
            anyhow::bail!(
                "invalid character '{}' in package name \"{}\": \
                 only alphanumeric, hyphen, and underscore are allowed",
                ch,
                pkg
            );
        }
    }
    Ok(())
}

/// Insert a package into a list. Returns true if inserted, false if already present.
pub fn insert(path: &Path, list: &NixList, pkg: &str) -> Result<bool> {
    validate_pkg_name(pkg)?;
    tracing::debug!(path = %path.display(), pkg, "inserting package");
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let (open, close) = find_list_range(&lines, list)?;

    // Check for duplicate
    for line in &lines[open + 1..close] {
        if let Some(name) = list.parse_item(line) {
            if name == pkg {
                tracing::debug!(pkg, "duplicate detected, skipping insert");
                return Ok(false);
            }
        }
    }

    // Insert before the closing line
    let new_line = list.format_item(pkg);
    lines.insert(close, new_line);

    atomic_write(path, &lines).with_context(|| format!("writing {}", path.display()))?;

    Ok(true)
}

/// Remove a package from a list. Returns true if removed, false if not found.
pub fn remove(path: &Path, list: &NixList, pkg: &str) -> Result<bool> {
    validate_pkg_name(pkg)?;
    tracing::debug!(path = %path.display(), pkg, "removing package");
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    let (open, close) = find_list_range(&lines, list)?;

    let found = lines[open + 1..close]
        .iter()
        .enumerate()
        .find(|(_, line)| list.parse_item(line).is_some_and(|name| name == pkg))
        .map(|(i, _)| open + 1 + i);

    match found {
        Some(idx) => {
            lines.remove(idx);
            atomic_write(path, &lines).with_context(|| format!("writing {}", path.display()))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Check whether any of the given package names is present in a list.
/// Reads the file once and checks all names in a single pass.
/// Returns the matched name, or None if no match.
pub fn contains_any(path: &Path, list: &NixList, names: &[&str]) -> Result<Option<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<String> = content.lines().map(String::from).collect();

    let (open, close) = match find_list_range(&lines, list) {
        Ok(range) => range,
        Err(_) => return Ok(None),
    };

    for line in &lines[open + 1..close] {
        if let Some(name) = list.parse_item(line) {
            if names.contains(&name.as_str()) {
                return Ok(Some(name));
            }
        }
    }
    Ok(None)
}

/// List all package names in a list within the given file.
/// Returns an empty vec if the list is not found (not every module has one).
pub fn list_packages(path: &Path, list: &NixList) -> Result<Vec<String>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<String> = content.lines().map(String::from).collect();

    let (open, close) = match find_list_range(&lines, list) {
        Ok(range) => range,
        Err(_) => return Ok(Vec::new()),
    };

    let mut pkgs = Vec::new();
    for line in &lines[open + 1..close] {
        if let Some(name) = list.parse_item(line) {
            pkgs.push(name);
        }
    }
    Ok(pkgs)
}

/// Write lines to a file atomically (temp file + fsync + rename).
fn atomic_write(path: &Path, lines: &[String]) -> Result<()> {
    tracing::trace!(path = %path.display(), "atomic write");
    let dir = path.parent().context("file has no parent directory")?;
    let content = lines.join("\n") + "\n";

    let mut tmp = NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

/// Write bytes to a file atomically (temp file + fsync + rename).
/// Use this instead of `std::fs::write` for any file that must survive power loss.
pub fn atomic_write_bytes(path: &Path, content: &[u8]) -> Result<()> {
    tracing::trace!(path = %path.display(), "atomic write bytes");
    let dir = path.parent().context("file has no parent directory")?;
    let mut tmp = NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}

/// Back up a file before editing. Returns the backup path.
pub fn backup(path: &Path) -> Result<std::path::PathBuf> {
    let backup_path = path.with_extension("nix.nex-backup");
    tracing::debug!(path = %path.display(), backup = %backup_path.display(), "backing up file");
    let dir = path.parent().context("file has no parent directory")?;
    let content = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut tmp = NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, &content)?;
    tmp.as_file().sync_all()?;
    tmp.persist(&backup_path)
        .with_context(|| format!("backing up {}", path.display()))?;
    Ok(backup_path)
}

/// Restore a file from its backup. Returns an error if the backup file is missing.
pub fn restore(path: &Path, backup_path: &Path) -> Result<()> {
    tracing::debug!(path = %path.display(), backup = %backup_path.display(), "restoring from backup");
    if !backup_path.exists() {
        anyhow::bail!(
            "backup file missing for {}: expected {}",
            path.display(),
            backup_path.display()
        );
    }
    fs::rename(backup_path, path).with_context(|| format!("restoring {}", path.display()))?;
    Ok(())
}

/// Delete a backup file.
pub fn delete_backup(backup_path: &Path) -> Result<()> {
    if backup_path.exists() {
        fs::remove_file(backup_path)?;
    }
    Ok(())
}

/// An edit session tracks backups for atomic multi-file operations.
pub struct EditSession {
    backups: Vec<(std::path::PathBuf, std::path::PathBuf)>, // (original, backup)
}

impl EditSession {
    pub fn new() -> Self {
        Self {
            backups: Vec::new(),
        }
    }

    /// Back up a file before editing. Idempotent per path.
    pub fn backup(&mut self, path: &Path) -> Result<()> {
        if self.backups.iter().any(|(p, _)| p == path) {
            return Ok(());
        }
        let bp = backup(path)?;
        self.backups.push((path.to_path_buf(), bp));
        Ok(())
    }

    /// Revert all edits by restoring backups.
    pub fn revert_all(&self) -> Result<()> {
        tracing::warn!(count = self.backups.len(), "reverting all edits");
        let mut errors = Vec::new();
        for (original, bp) in &self.backups {
            if let Err(e) = restore(original, bp) {
                errors.push(format!("{}: {e}", original.display()));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("failed to revert some files:\n  {}", errors.join("\n  "))
        }
    }

    /// Commit all edits by deleting backups.
    pub fn commit_all(&self) -> Result<()> {
        tracing::debug!(count = self.backups.len(), "committing all edits");
        for (_, bp) in &self.backups {
            delete_backup(bp)?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn has_changes(&self) -> bool {
        !self.backups.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nixfile;
    use tempfile::TempDir;

    fn write_fixture(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).expect("write fixture");
        path
    }

    const BASE_NIX: &str = r#"{ pkgs, username, ... }:

{
  home.packages = with pkgs; [
    ## Shell
    bash
    git
    vim
  ];
}
"#;

    const BREW_NIX: &str = r#"{ ... }:

{
  homebrew = {
    brews = [
      "rustup"
      "esptool"
    ];

    casks = [
      "firefox"
      "kitty"
    ];
  };
}
"#;

    #[test]
    fn test_contains_bare() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "base.nix", BASE_NIX);
        assert!(contains(&path, &nixfile::NIX_PACKAGES, "bash").expect("contains"));
        assert!(contains(&path, &nixfile::NIX_PACKAGES, "vim").expect("contains"));
        assert!(!contains(&path, &nixfile::NIX_PACKAGES, "htop").expect("contains"));
    }

    #[test]
    fn test_contains_quoted() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "brew.nix", BREW_NIX);
        assert!(contains(&path, &nixfile::HOMEBREW_BREWS, "rustup").expect("contains"));
        assert!(!contains(&path, &nixfile::HOMEBREW_BREWS, "qemu").expect("contains"));
        assert!(contains(&path, &nixfile::HOMEBREW_CASKS, "firefox").expect("contains"));
        assert!(!contains(&path, &nixfile::HOMEBREW_CASKS, "slack").expect("contains"));
    }

    #[test]
    fn test_insert_bare() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "base.nix", BASE_NIX);
        assert!(insert(&path, &nixfile::NIX_PACKAGES, "htop").expect("insert"));
        assert!(contains(&path, &nixfile::NIX_PACKAGES, "htop").expect("contains"));
        // Duplicate returns false
        assert!(!insert(&path, &nixfile::NIX_PACKAGES, "htop").expect("insert dup"));
    }

    #[test]
    fn test_insert_quoted() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "brew.nix", BREW_NIX);
        assert!(insert(&path, &nixfile::HOMEBREW_CASKS, "slack").expect("insert"));
        assert!(contains(&path, &nixfile::HOMEBREW_CASKS, "slack").expect("contains"));
    }

    #[test]
    fn test_remove_bare() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "base.nix", BASE_NIX);
        assert!(remove(&path, &nixfile::NIX_PACKAGES, "vim").expect("remove"));
        assert!(!contains(&path, &nixfile::NIX_PACKAGES, "vim").expect("contains"));
        // Remove non-existent returns false
        assert!(!remove(&path, &nixfile::NIX_PACKAGES, "vim").expect("remove again"));
    }

    #[test]
    fn test_remove_quoted() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "brew.nix", BREW_NIX);
        assert!(remove(&path, &nixfile::HOMEBREW_BREWS, "esptool").expect("remove"));
        assert!(!contains(&path, &nixfile::HOMEBREW_BREWS, "esptool").expect("contains"));
    }

    #[test]
    fn test_list_packages() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "base.nix", BASE_NIX);
        let pkgs = list_packages(&path, &nixfile::NIX_PACKAGES).expect("list");
        assert_eq!(pkgs, vec!["bash", "git", "vim"]);
    }

    #[test]
    fn test_list_packages_quoted() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "brew.nix", BREW_NIX);
        let brews = list_packages(&path, &nixfile::HOMEBREW_BREWS).expect("list");
        assert_eq!(brews, vec!["rustup", "esptool"]);
        let casks = list_packages(&path, &nixfile::HOMEBREW_CASKS).expect("list");
        assert_eq!(casks, vec!["firefox", "kitty"]);
    }

    #[test]
    fn test_edit_session_revert() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_fixture(dir.path(), "base.nix", BASE_NIX);

        let mut session = EditSession::new();
        session.backup(&path).expect("backup");

        insert(&path, &nixfile::NIX_PACKAGES, "htop").expect("insert");
        assert!(contains(&path, &nixfile::NIX_PACKAGES, "htop").expect("contains"));

        session.revert_all().expect("revert");
        assert!(!contains(&path, &nixfile::NIX_PACKAGES, "htop").expect("contains after revert"));
    }
}
