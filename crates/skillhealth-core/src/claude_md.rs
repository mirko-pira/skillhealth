use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const MAX_IMPORT_DEPTH: u8 = 5;

#[derive(Debug, Serialize)]
pub struct ClaudeMdInfo {
    pub path: PathBuf,
    /// Top-level file content (imports NOT inlined — used for graph/drift scans).
    #[serde(skip)]
    pub content: String,
    /// Total chars including recursively resolved @imports.
    pub resolved_chars: usize,
    pub missing_imports: Vec<String>,
}

/// Project CLAUDE.md files walking up from cwd, plus the global <config>/CLAUDE.md.
pub fn collect(config_dir: &Path, cwd: &Path) -> Vec<ClaudeMdInfo> {
    let mut out = Vec::new();
    let mut cur = Some(cwd);
    while let Some(p) = cur {
        let cand = p.join("CLAUDE.md");
        if cand.is_file() {
            out.push(resolve(&cand));
        }
        cur = p.parent();
    }
    let global = config_dir.join("CLAUDE.md");
    if global.is_file() {
        out.push(resolve(&global));
    }
    out
}

pub fn resolve(path: &Path) -> ClaudeMdInfo {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut visited: HashSet<PathBuf> = HashSet::new();
    visited.insert(path.to_path_buf());
    let mut missing = Vec::new();
    let resolved_chars = resolve_inner(path, &content, 0, &mut visited, &mut missing);
    ClaudeMdInfo {
        path: path.to_path_buf(),
        content,
        resolved_chars,
        missing_imports: missing,
    }
}

fn resolve_inner(
    path: &Path,
    content: &str,
    depth: u8,
    visited: &mut HashSet<PathBuf>,
    missing: &mut Vec<String>,
) -> usize {
    let mut total = content.chars().count();
    if depth >= MAX_IMPORT_DEPTH {
        return total;
    }
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for token in line.split_whitespace() {
            let Some(raw) = token.strip_prefix('@') else {
                continue;
            };
            if !is_import_path(raw) {
                continue;
            }
            let target = expand_path(raw, path);
            match std::fs::read_to_string(&target) {
                Ok(imported) => {
                    if visited.insert(target.clone()) {
                        total += resolve_inner(&target, &imported, depth + 1, visited, missing);
                    }
                }
                Err(_) => missing.push(raw.to_string()),
            }
        }
    }
    total
}

fn is_import_path(raw: &str) -> bool {
    !raw.is_empty()
        && (raw.starts_with('~')
            || raw.starts_with('/')
            || raw.starts_with('.')
            || raw.ends_with(".md"))
}

fn expand_path(raw: &str, relative_to: &Path) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        relative_to.parent().unwrap_or(Path::new(".")).join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_imports_recursively_and_counts_chars() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("CLAUDE.md"), "Main 10ch\n@docs/a.md\n").unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("a.md"), "AAAA\n@b.md\n").unwrap();
        fs::write(root.join("docs").join("b.md"), "BB").unwrap();
        let info = resolve(&root.join("CLAUDE.md"));
        let expected = "Main 10ch\n@docs/a.md\n".chars().count()
            + "AAAA\n@b.md\n".chars().count()
            + "BB".chars().count();
        assert_eq!(info.resolved_chars, expected);
        assert!(info.missing_imports.is_empty());
    }

    #[test]
    fn missing_import_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "@docs/missing.md\n").unwrap();
        let info = resolve(&tmp.path().join("CLAUDE.md"));
        assert_eq!(info.missing_imports, vec!["docs/missing.md".to_string()]);
    }

    #[test]
    fn imports_in_code_fences_and_emails_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("CLAUDE.md"),
            "Contact dev@example.com and @anthropic-ai/claude-code\n```\n@docs/fenced.md\n```\n",
        )
        .unwrap();
        let info = resolve(&tmp.path().join("CLAUDE.md"));
        assert!(info.missing_imports.is_empty());
    }

    #[test]
    fn import_cycles_terminate() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("CLAUDE.md"), "@a.md").unwrap();
        fs::write(tmp.path().join("a.md"), "@CLAUDE.md").unwrap();
        let info = resolve(&tmp.path().join("CLAUDE.md"));
        assert!(info.resolved_chars > 0); // just: it returns
    }

    #[test]
    fn collect_finds_project_walkup_and_config_claude_md() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let cwd = tmp.path().join("repo").join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::write(tmp.path().join("repo").join("CLAUDE.md"), "project").unwrap();
        fs::write(config.join("CLAUDE.md"), "global").unwrap();
        let found = collect(&config, &cwd);
        assert_eq!(found.len(), 2);
    }
}
