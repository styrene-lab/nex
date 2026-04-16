use console::style;

#[allow(dead_code)]
pub fn added(pkg: &str) {
    eprintln!("  {} {}", style("+").green().bold(), pkg);
}

pub fn added_with_source(pkg: &str, source: &str) {
    eprintln!(
        "  {} {} {}",
        style("+").green().bold(),
        pkg,
        style(format!("({source})")).dim()
    );
}

pub fn removed(pkg: &str) {
    eprintln!("  {} {}", style("-").red().bold(), pkg);
}

pub fn already(pkg: &str) {
    eprintln!("  {} {} (already present)", style("=").yellow(), pkg);
}

pub fn not_found(pkg: &str, hint: &str) {
    eprintln!("  {} {} — {}", style("?").red(), pkg, hint);
}

pub fn status(action: &str) {
    eprintln!("{}", style(format!(">>> {action}")).cyan().bold());
}

pub fn error(msg: &str) {
    eprintln!("{} {}", style("error:").red().bold(), msg);
}

pub fn dry_run(msg: &str) {
    eprintln!("{} {}", style("[dry-run]").magenta(), msg);
}
