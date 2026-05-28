use std::io::IsTerminal;

/// Returns true when nex should emit ANSI styling on stdout.
pub fn should_color_stdout() -> bool {
    should_color(std::io::stdout().is_terminal())
}

/// Returns true when nex should emit ANSI styling on stderr.
pub fn should_color_stderr() -> bool {
    should_color(std::io::stderr().is_terminal())
}

fn should_color(stream_is_terminal: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("NEX_NO_COLOR").is_some() {
        return false;
    }
    if matches!(
        std::env::var("NEX_COLOR").as_deref(),
        Ok("always" | "1" | "true" | "yes")
    ) {
        return true;
    }
    stream_is_terminal
}

/// Apply process-wide color policy for crates that honor `console`'s global toggles.
pub fn configure_console_colors() {
    console::set_colors_enabled(should_color_stdout());
    console::set_colors_enabled_stderr(should_color_stderr());
}

/// Convert terminal/PTY capture into text safe for logs, parsers, JSON, and integrations.
///
/// This strips ANSI escape/control sequences, resolves common carriage-return line
/// rewrites, and applies backspace overstrikes. Newlines and tabs are preserved.
pub fn sanitize_terminal_capture(input: &str) -> String {
    normalize_terminal_rewrites(&strip_ansi(input))
}

/// Strip ANSI escape/control sequences from text captured from terminals or PTYs.
///
/// This intentionally handles the common terminal families that leak into logs and
/// machine-readable integration surfaces: CSI (`ESC [`), OSC (`ESC ] ... BEL/ST`),
/// DCS/PM/APC (`ESC P/^/_ ... ST`), single-character ESC commands, and C1 CSI.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => strip_esc_sequence(&mut chars),
            '\u{009b}' => strip_csi(&mut chars),
            '\u{009d}' => strip_until_c1_terminator(&mut chars),
            _ => out.push(ch),
        }
    }

    out
}

fn normalize_terminal_rewrites(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut line_start = 0usize;

    for ch in input.chars() {
        match ch {
            '\r' => out.truncate(line_start),
            '\n' => {
                out.push('\n');
                line_start = out.len();
            }
            '\u{0008}' => {
                if out.len() > line_start {
                    out.pop();
                }
            }
            '\t' => out.push(ch),
            ch if ch.is_control() => {}
            _ => out.push(ch),
        }
    }

    out
}

fn strip_esc_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    match chars.next() {
        Some('[') => strip_csi(chars),
        Some(']') => strip_until_string_terminator(chars),
        Some('P' | '^' | '_' | 'X') => strip_until_string_terminator(chars),
        Some('(' | ')' | '*' | '+' | '-' | '.' | '/') => {
            let _ = chars.next();
        }
        Some(_) | None => {}
    }
}

fn strip_csi<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn strip_until_string_terminator<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    while let Some(ch) = chars.next() {
        if ch == '\u{0007}' || ch == '\u{009c}' {
            break;
        }
        if ch == '\u{1b}' && matches!(chars.peek(), Some('\\')) {
            let _ = chars.next();
            break;
        }
    }
}

fn strip_until_c1_terminator<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    for ch in chars.by_ref() {
        if ch == '\u{0007}' || ch == '\u{009c}' {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_terminal_capture, strip_ansi};

    #[test]
    fn strips_sgr_sequences() {
        assert_eq!(strip_ansi("\u{1b}[32mgreen\u{1b}[0m"), "green");
    }

    #[test]
    fn strips_cursor_and_line_control_sequences() {
        assert_eq!(strip_ansi("a\u{1b}[?25lb\u{1b}[2Kc"), "abc");
    }

    #[test]
    fn strips_osc_sequences_terminated_by_bel_or_st() {
        assert_eq!(strip_ansi("x\u{1b}]0;title\u{0007}y"), "xy");
        assert_eq!(
            strip_ansi("x\u{1b}]8;;https://example.test\u{1b}\\link\u{1b}]8;;\u{1b}\\y"),
            "xlinky"
        );
    }

    #[test]
    fn strips_single_character_escape_sequences() {
        assert_eq!(strip_ansi("a\u{1b}7b\u{1b}8c"), "abc");
    }

    #[test]
    fn sanitize_resolves_carriage_return_rewrites() {
        assert_eq!(
            sanitize_terminal_capture("build 10%\rbuild 100%\n"),
            "build 100%\n"
        );
    }

    #[test]
    fn sanitize_applies_backspace_overstrikes() {
        assert_eq!(sanitize_terminal_capture("abc\u{0008}\u{0008}XY"), "aXY");
    }

    #[test]
    fn sanitize_removes_non_whitespace_control_characters() {
        assert_eq!(sanitize_terminal_capture("a\u{0000}b\tc\n"), "ab\tc\n");
    }
}
