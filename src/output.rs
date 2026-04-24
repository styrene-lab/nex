use console::style;

#[allow(dead_code)]
pub fn added(pkg: &str) {
    tracing::info!(pkg, "package added");
    eprintln!("  {} {}", style("+").green().bold(), pkg);
}

pub fn added_with_source(pkg: &str, source: &str) {
    tracing::info!(pkg, source, "package added");
    eprintln!(
        "  {} {} {}",
        style("+").green().bold(),
        pkg,
        style(format!("({source})")).dim()
    );
}

pub fn removed(pkg: &str) {
    tracing::info!(pkg, "package removed");
    eprintln!("  {} {}", style("-").red().bold(), pkg);
}

pub fn already(pkg: &str) {
    tracing::debug!(pkg, "package already present");
    eprintln!("  {} {} (already present)", style("=").yellow(), pkg);
}

pub fn not_found(pkg: &str, hint: &str) {
    tracing::warn!(pkg, hint, "package not found");
    eprintln!("  {} {} — {}", style("?").red(), pkg, hint);
}

pub fn status(action: &str) {
    tracing::info!(action, "status");
    eprintln!("{}", style(format!(">>> {action}")).cyan().bold());
}

pub fn warn(msg: &str) {
    tracing::warn!("{}", msg);
    eprintln!("{} {}", style("warning:").yellow().bold(), msg);
}

pub fn error(msg: &str) {
    tracing::error!("{}", msg);
    eprintln!("{} {}", style("error:").red().bold(), msg);
}

pub fn dry_run(msg: &str) {
    tracing::info!(msg, "dry-run");
    eprintln!("{} {}", style("[dry-run]").magenta(), msg);
}
