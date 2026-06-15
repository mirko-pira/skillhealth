use base64::Engine;
use ratatui::DefaultTerminal;
use skillhealth_core::report::Report;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// $EDITOR → $VISUAL → vi. Empty strings count as unset.
fn resolve_editor(editor: Option<String>, visual: Option<String>) -> String {
    [editor, visual]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "vi".to_string())
}

/// Run through `sh -c` so values like "code -w" keep their flags; the path
/// is single-quoted (with embedded-quote escaping) so spaces survive.
fn editor_invocation(editor: &str, path: &Path) -> (String, Vec<String>) {
    let quoted = format!("'{}'", path.display().to_string().replace('\'', r"'\''"));
    (
        "sh".to_string(),
        vec!["-c".to_string(), format!("{editor} {quoted}")],
    )
}

/// Suspend the TUI (cooked mode, main screen), run the editor, re-enter.
/// The caller has already dropped its EventStream and rebuilds it after.
pub fn open_editor(terminal: &mut DefaultTerminal, path: &Path) -> anyhow::Result<()> {
    let editor = resolve_editor(std::env::var("EDITOR").ok(), std::env::var("VISUAL").ok());
    let (program, args) = editor_invocation(&editor, path);

    ratatui::restore();
    let status = Command::new(&program).args(&args).status();
    *terminal = ratatui::try_init()?;

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => anyhow::bail!("'{editor}' exited with {s}"),
        Err(e) => anyhow::bail!("could not launch '{editor}': {e}"),
    }
}

pub fn open_graph(report: &Report) -> Result<(), String> {
    let path = crate::render::html::write_dashboard(report).map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| e.to_string())
}

/// OSC 52 escape — terminals that support it set the system clipboard, even
/// over SSH. Harmless bytes on terminals that don't.
fn osc52(text: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    format!("\x1b]52;c;{b64}\x07")
}

/// Best-effort, never fails the UI: OSC 52 straight to the controlling
/// terminal, plus the first native clipboard tool that accepts stdin
/// (pbcopy on macOS, xclip/wl-copy on Linux).
pub fn copy_to_clipboard(text: &str) {
    if let Ok(mut tty) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
        let _ = tty.write_all(osc52(text).as_bytes());
        let _ = tty.flush();
    }
    let tools: &[(&str, &[&str])] = &[
        ("pbcopy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("wl-copy", &[]),
    ];
    for (tool, args) in tools {
        let spawned = Command::new(tool)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if matches!(child.wait(), Ok(s) if s.success()) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn editor_resolution_prefers_editor_then_visual_then_vi() {
        assert_eq!(
            resolve_editor(Some("nvim".into()), Some("code -w".into())),
            "nvim"
        );
        assert_eq!(resolve_editor(None, Some("code -w".into())), "code -w");
        assert_eq!(resolve_editor(None, None), "vi");
        assert_eq!(resolve_editor(Some(String::new()), None), "vi"); // empty = unset
    }

    #[test]
    fn editor_invocation_goes_through_sh_to_honor_flags() {
        let (program, args) = editor_invocation("code -w", Path::new("/tmp/a b/SKILL.md"));
        assert_eq!(program, "sh");
        assert_eq!(args[0], "-c");
        assert!(args[1].starts_with("code -w "));
        assert!(args[1].contains("'/tmp/a b/SKILL.md'")); // quoted: spaces survive
    }

    #[test]
    fn osc52_payload_wraps_base64() {
        let seq = osc52("echo hi");
        assert!(seq.starts_with("\x1b]52;c;"));
        assert!(seq.ends_with('\x07'));
        assert!(seq.contains("ZWNobyBoaQ==")); // base64("echo hi")
    }
}
