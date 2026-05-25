use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

pub fn evaluate_json(path: &Path) -> Result<serde_json::Value> {
    let output = run_eval_json(path)?;
    serde_json::from_slice(&output)
        .with_context(|| format!("Pkl evaluator did not emit JSON for {}", path.display()))
}

fn run_eval_json(path: &Path) -> Result<Vec<u8>> {
    let path_arg = path.to_string_lossy().to_string();
    let mut attempts = Vec::new();

    if let Ok(bin) = std::env::var("NEX_PKL") {
        attempts.push(PklCommand {
            program: bin,
            args: vec![
                "eval".into(),
                "--format".into(),
                "json".into(),
                path_arg.clone(),
            ],
        });
    }
    attempts.push(PklCommand {
        program: "pkl".into(),
        args: vec![
            "eval".into(),
            "--format".into(),
            "json".into(),
            path_arg.clone(),
        ],
    });
    attempts.push(PklCommand {
        program: "nix".into(),
        args: vec![
            "shell".into(),
            "nixpkgs#pkl".into(),
            "-c".into(),
            "pkl".into(),
            "eval".into(),
            "--format".into(),
            "json".into(),
            path_arg,
        ],
    });

    let mut missing = Vec::new();
    for attempt in attempts {
        let output = Command::new(&attempt.program)
            .args(&attempt.args)
            .stdin(Stdio::null())
            .output();
        match output {
            Ok(output) if output.status.success() => return Ok(output.stdout),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!(
                    "Pkl evaluator command failed: {} {}\n{}",
                    attempt.program,
                    attempt.args.join(" "),
                    stderr.trim()
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => missing.push(attempt.program),
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "starting Pkl evaluator command: {} {}",
                        attempt.program,
                        attempt.args.join(" ")
                    )
                });
            }
        }
    }

    bail!(
        "Pkl evaluator unavailable; install `pkl`, set NEX_PKL, or provide `nix` so Nex can run nixpkgs#pkl (tried: {})",
        missing.join(", ")
    )
}

struct PklCommand {
    program: String,
    args: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    #[cfg(unix)]
    fn evaluates_json_with_nex_pkl_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_pkl = temp.path().join("fake-pkl");
        fs::write(
            &fake_pkl,
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"schema":"ok"}
JSON
"#,
        )
        .expect("write fake pkl");
        let mut perms = fs::metadata(&fake_pkl).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_pkl, perms).expect("chmod");

        let old = std::env::var_os("NEX_PKL");
        std::env::set_var("NEX_PKL", &fake_pkl);
        let json = evaluate_json(&temp.path().join("source.pkl")).expect("evaluate pkl");
        if let Some(old) = old {
            std::env::set_var("NEX_PKL", old);
        } else {
            std::env::remove_var("NEX_PKL");
        }

        assert_eq!(json["schema"], "ok");
    }
}
