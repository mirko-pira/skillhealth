use crate::model::{Skill, SkillSource};
use crate::parse::parse_skill_md;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct DiscoverInput {
    pub config_dir: PathBuf,
    pub cwd: PathBuf,
}

#[derive(Debug, Default, Serialize)]
pub struct Discovery {
    pub skills: Vec<Skill>,
    /// Directories inside a skills root that have no SKILL.md.
    pub debris: Vec<PathBuf>,
}

pub fn discover(input: &DiscoverInput) -> Discovery {
    let mut d = Discovery::default();
    scan_skills_dir(&input.config_dir.join("skills"), SkillSource::User, &mut d);
    for dir in project_skill_dirs(&input.cwd, &input.config_dir) {
        scan_skills_dir(&dir, SkillSource::Project, &mut d);
    }
    scan_plugins(&input.config_dir.join("plugins"), &mut d);
    // Global dedup BY CANONICAL PATH: defends against the same SKILL.md reached
    // via two different paths (e.g. symlinked skills dirs). Keep the first
    // occurrence (user > project > plugin ordering). Deliberately NOT keyed on
    // name: distinct paths sharing a name are genuine cross-source shadowing,
    // which doctor must see to emit W002.
    let mut seen = std::collections::HashSet::new();
    d.skills.retain(|s| {
        let key = std::fs::canonicalize(&s.path).unwrap_or_else(|_| s.path.clone());
        seen.insert(key)
    });
    d
}

impl Discovery {
    /// Mark skills whose plugin is switched off (enabledPlugins=false).
    pub fn apply_disabled(&mut self, disabled: &std::collections::BTreeSet<String>) {
        for s in &mut self.skills {
            if let SkillSource::Plugin(p) = &s.source {
                s.disabled = disabled.contains(p);
            }
        }
    }
}

/// `.claude/skills` dirs walking up from `cwd`, stopping at the git root
/// (inclusive): a project's skills live inside its repo, so anything above
/// the first `.git` (dir, or file for worktrees/submodules) is not ours.
/// The config root's own skills dir is never a project root — it's already
/// scanned as User (matters when cwd is outside any repo and the walk-up
/// climbs past ~/.claude).
/// Public so the TUI watcher can watch the same roots discovery scans.
pub fn project_skill_dirs(cwd: &Path, config_dir: &Path) -> Vec<PathBuf> {
    let config_skills = canonical_or_self(&config_dir.join("skills"));
    let mut out = Vec::new();
    let mut cur = Some(cwd);
    while let Some(p) = cur {
        let cand = p.join(".claude").join("skills");
        if cand.is_dir() && canonical_or_self(&cand) != config_skills {
            out.push(cand);
        }
        if p.join(".git").exists() {
            break;
        }
        cur = p.parent();
    }
    out
}

fn canonical_or_self(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The project boundary for `cwd`: the first ancestor (inclusive) containing
/// `.git` (dir, or file for worktrees/submodules). Fallback when no git root
/// exists: the deepest ancestor owning a non-config `.claude/skills`. None =
/// not inside a project. This is the dir scope/lens hang off — its file_name
/// is the header label.
pub fn project_root(cwd: &Path, config_dir: &Path) -> Option<PathBuf> {
    let config_skills = canonical_or_self(&config_dir.join("skills"));
    let mut skills_owner: Option<PathBuf> = None;
    let mut cur = Some(cwd);
    while let Some(p) = cur {
        if p.join(".git").exists() {
            return Some(p.to_path_buf());
        }
        let cand = p.join(".claude").join("skills");
        if skills_owner.is_none() && cand.is_dir() && canonical_or_self(&cand) != config_skills {
            skills_owner = Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    skills_owner
}

fn scan_skills_dir(dir: &Path, source: SkillSource, d: &mut Discovery) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if skill_md.is_file() {
            if let Some(skill) = load_skill(&skill_md, source.clone(), None) {
                d.skills.push(skill);
            }
        } else {
            d.debris.push(path);
        }
    }
}

fn scan_plugins(plugins_dir: &Path, d: &mut Discovery) {
    // v1 supported plugin layouts:
    //   1. plugins/cache/<org>/<plugin>[/<version>]/skills/<dir>/SKILL.md
    //      (Claude Code installed-plugin layout; version dir optional)
    //   2. plugins/<plugin>/skills/<dir>/SKILL.md
    //      (direct layout — covers manual/simple installs)
    //
    // NOT supported in v1 (scanning them caused the original 71s + over-count):
    //   - plugins/repos/**    (source trees, not installed plugins)
    //   - plugins/marketplaces/**  (source trees, not installed plugins)

    // --- Layout 1: cache/ fast path ---
    let cache_dir = plugins_dir.join("cache");
    if cache_dir.is_dir() {
        let Ok(orgs) = std::fs::read_dir(&cache_dir) else {
            return;
        };
        for org_entry in orgs.flatten() {
            let org_path = org_entry.path();
            if !org_path.is_dir() {
                continue;
            }
            let Ok(plugins) = std::fs::read_dir(&org_path) else {
                continue;
            };
            for plugin_entry in plugins.flatten() {
                let plugin_path = plugin_entry.path();
                if !plugin_path.is_dir() {
                    continue;
                }
                let plugin_name = plugin_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                scan_plugin_dir(&plugin_path, &plugin_name, d);
            }
        }
    }

    // --- Layout 2: direct plugins/<plugin>/skills/ (non-cache, non-reserved dirs) ---
    // Skipped dirs: cache (handled above), repos and marketplaces (source trees).
    let Ok(children) = std::fs::read_dir(plugins_dir) else {
        return;
    };
    for child_entry in children.flatten() {
        let child_path = child_entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let dir_name = child_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if matches!(dir_name.as_str(), "cache" | "repos" | "marketplaces") {
            continue;
        }
        let skills_dir = child_path.join("skills");
        if skills_dir.is_dir() {
            let before = d.skills.len();
            scan_skills_dir(&skills_dir, SkillSource::User, d);
            retag_skills_as_plugin(&mut d.skills[before..], &dir_name);
        }
    }
}

/// Scan one plugin directory: looks for skills/ directly or under version subdirectories.
/// When multiple version dirs each contain the same skill name, the version dir with the
/// most-recent mtime wins; ties (or unavailable mtime) keep the first encountered.
fn scan_plugin_dir(plugin_path: &Path, plugin_name: &str, d: &mut Discovery) {
    let direct_skills = plugin_path.join("skills");
    if direct_skills.is_dir() {
        let before = d.skills.len();
        scan_skills_dir(&direct_skills, SkillSource::User, d);
        retag_skills_as_plugin(&mut d.skills[before..], plugin_name);
        return;
    }

    // Collect version subdirs sorted by mtime descending (newest first).
    // This ensures that when we dedup by full name, we keep the skill from the newest version.
    let Ok(versions) = std::fs::read_dir(plugin_path) else {
        return;
    };
    let mut ver_dirs: Vec<(std::time::SystemTime, std::path::PathBuf)> = versions
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (mtime, e.path())
        })
        .collect();
    ver_dirs.sort_by_key(|b| std::cmp::Reverse(b.0)); // newest first

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, ver_path) in ver_dirs {
        let skills_dir = ver_path.join("skills");
        if !skills_dir.is_dir() {
            continue;
        }
        // Load skills from this version dir into a temporary buffer, retag, then dedup-merge.
        let mut tmp = Discovery::default();
        scan_skills_dir(&skills_dir, SkillSource::User, &mut tmp);
        retag_skills_as_plugin(&mut tmp.skills, plugin_name);
        for skill in tmp.skills {
            if seen.insert(skill.name.clone()) {
                d.skills.push(skill);
            }
        }
        // Debris from version subdirs is intentionally not propagated (not meaningful here).
    }
}

fn retag_skills_as_plugin(skills: &mut [Skill], plugin_name: &str) {
    for skill in skills.iter_mut() {
        skill.name = format!("{plugin_name}:{}", skill.dir_name);
        skill.source = SkillSource::Plugin(plugin_name.to_string());
    }
}

fn load_skill(skill_md: &Path, source: SkillSource, plugin: Option<&str>) -> Option<Skill> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let parsed = parse_skill_md(&content);
    let dir_name = skill_md
        .parent()?
        .file_name()?
        .to_string_lossy()
        .into_owned();
    let (name, source) = match plugin {
        Some(p) => (
            format!("{p}:{dir_name}"),
            SkillSource::Plugin(p.to_string()),
        ),
        None => (dir_name.clone(), source),
    };
    let est_tokens = parsed.body.chars().count() / 4;
    let always_on_tokens = parsed
        .description
        .as_ref()
        .map(|d| d.chars().count() / 4)
        .unwrap_or(0);
    Some(Skill {
        name,
        dir_name,
        source,
        path: skill_md.to_path_buf(),
        description: parsed.description,
        frontmatter_ok: parsed.frontmatter_ok,
        body: parsed.body,
        est_tokens,
        always_on_tokens,
        disabled: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skill(root: &std::path::Path, dir: &str, name: &str) {
        let d = root.join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test skill {name}\n---\nBody of {name}."),
        )
        .unwrap();
    }

    #[test]
    fn discovers_user_project_plugin_and_debris() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let cwd = tmp.path().join("repo").join("sub");
        fs::create_dir_all(&cwd).unwrap();

        // user skill
        write_skill(&config.join("skills"), "cfo", "cfo");
        // debris: dir without SKILL.md
        fs::create_dir_all(config.join("skills").join("leftover")).unwrap();
        fs::write(
            config.join("skills").join("leftover").join("notes.txt"),
            "x",
        )
        .unwrap();
        // project skill, one level above cwd (walk-up)
        write_skill(
            &tmp.path().join("repo").join(".claude").join("skills"),
            "deploy",
            "deploy",
        );
        // plugin skill with version dir in path
        write_skill(
            &config
                .join("plugins")
                .join("cache")
                .join("mp")
                .join("superpowers")
                .join("5.1.0")
                .join("skills"),
            "writing-plans",
            "writing-plans",
        );

        let d = discover(&DiscoverInput {
            config_dir: config.clone(),
            cwd,
        });

        let names: Vec<&str> = d.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"cfo"));
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"superpowers:writing-plans"));
        assert_eq!(d.debris.len(), 1);
        assert!(d.debris[0].ends_with("leftover"));

        let plugin = d
            .skills
            .iter()
            .find(|s| s.name == "superpowers:writing-plans")
            .unwrap();
        assert_eq!(plugin.source, SkillSource::Plugin("superpowers".into()));
        assert_eq!(plugin.dir_name, "writing-plans");
        let proj = d.skills.iter().find(|s| s.name == "deploy").unwrap();
        assert_eq!(proj.source, SkillSource::Project);
    }

    #[test]
    fn missing_roots_yield_empty_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let d = discover(&DiscoverInput {
            config_dir: tmp.path().join("nope"),
            cwd: tmp.path().to_path_buf(),
        });
        assert!(d.skills.is_empty());
        assert!(d.debris.is_empty());
    }

    #[test]
    fn broken_frontmatter_still_discovered() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let dir = config.join("skills").join("broken");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "no frontmatter at all").unwrap();
        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        assert_eq!(d.skills.len(), 1);
        assert!(!d.skills[0].frontmatter_ok);
        assert_eq!(d.skills[0].name, "broken"); // falls back to dir name
    }

    // --- Regression 2: constrain v1 supported plugin layouts ---

    #[test]
    fn direct_plugin_layout_discovered() {
        // plugins/foo/skills/bar/SKILL.md → name "foo:bar", source Plugin("foo")
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        write_skill(
            &config.join("plugins").join("foo").join("skills"),
            "bar",
            "bar",
        );
        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        let skill = d.skills.iter().find(|s| s.name == "foo:bar");
        assert!(skill.is_some(), "expected foo:bar to be discovered");
        assert_eq!(skill.unwrap().source, SkillSource::Plugin("foo".into()));
    }

    #[test]
    fn cache_plugin_no_version_dir_discovered() {
        // plugins/cache/org/plug/skills/x/SKILL.md (no version dir) → "plug:x"
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        write_skill(
            &config
                .join("plugins")
                .join("cache")
                .join("org")
                .join("plug")
                .join("skills"),
            "x",
            "x",
        );
        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        assert!(
            d.skills.iter().any(|s| s.name == "plug:x"),
            "expected plug:x to be discovered; got: {:?}",
            d.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn multi_version_dir_dedup() {
        // plugins/cache/org/plug/1.0.0/skills/x/SKILL.md
        // + plugins/cache/org/plug/2.0.0/skills/x/SKILL.md
        // → exactly ONE "plug:x"
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let base = config
            .join("plugins")
            .join("cache")
            .join("org")
            .join("plug");
        write_skill(&base.join("1.0.0").join("skills"), "x", "x");
        write_skill(&base.join("2.0.0").join("skills"), "x", "x");
        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        let count = d.skills.iter().filter(|s| s.name == "plug:x").count();
        assert_eq!(count, 1, "expected exactly 1 plug:x, got {count}");
    }

    // --- Regression 3: dedup must be by path, not name — shadowing is a real signal ---

    #[test]
    fn cross_source_shadowing_returns_both_entries() {
        // user skill `dup` AND project skill `dup` (distinct SKILL.md paths):
        // discover() must return BOTH so doctor W002 (shadowing) can fire.
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let cwd = tmp.path().join("repo").join("sub");
        fs::create_dir_all(&cwd).unwrap();
        write_skill(&config.join("skills"), "dup", "dup");
        write_skill(
            &tmp.path().join("repo").join(".claude").join("skills"),
            "dup",
            "dup",
        );

        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd,
        });

        let dups: Vec<_> = d.skills.iter().filter(|s| s.name == "dup").collect();
        assert_eq!(
            dups.len(),
            2,
            "expected BOTH user and project 'dup' (shadowing must be observable); got {:?}",
            d.skills
                .iter()
                .map(|s| (&s.name, &s.path))
                .collect::<Vec<_>>()
        );
        assert!(dups.iter().any(|s| s.source == SkillSource::User));
        assert!(dups.iter().any(|s| s.source == SkillSource::Project));
    }

    #[test]
    fn walkup_rediscovery_of_config_skills_is_deduped() {
        // cwd walk-up reaches the config dir's parent and re-finds <config>/skills:
        // the SAME SKILL.md path is scanned twice → must be returned exactly ONCE.
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let config = home.join(".claude");
        let cwd = home.join("projects").join("x");
        fs::create_dir_all(&cwd).unwrap();
        write_skill(&config.join("skills"), "dup", "dup");

        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd,
        });

        let count = d.skills.iter().filter(|s| s.name == "dup").count();
        assert_eq!(
            count, 1,
            "same canonical path discovered twice must dedup to 1, got {count}"
        );
    }

    // --- v0.2 walk-up boundary: project skills live inside the repo ---

    #[test]
    fn walkup_stops_at_git_root() {
        // outer/.claude/skills sits ABOVE the git root: it belongs to some other
        // context (e.g. $HOME), not to this project — must NOT be discovered.
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let outer = tmp.path().join("outer");
        let repo = outer.join("repo");
        let cwd = repo.join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        write_skill(&outer.join(".claude").join("skills"), "outer", "outer");
        write_skill(&repo.join(".claude").join("skills"), "inner", "inner");

        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd,
        });

        let names: Vec<&str> = d.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"inner"),
            "git root's own skills are project skills"
        );
        assert!(
            !names.contains(&"outer"),
            "skills above the git root must not be discovered; got {names:?}"
        );
    }

    #[test]
    fn walkup_stops_at_git_file_boundary() {
        // Worktrees and submodules mark the root with a .git FILE, not a dir.
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let outer = tmp.path().join("outer");
        let repo = outer.join("wt");
        let cwd = repo.join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::write(repo.join(".git"), "gitdir: /elsewhere/.git/worktrees/wt").unwrap();
        write_skill(&outer.join(".claude").join("skills"), "outer", "outer");

        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd,
        });

        assert!(
            d.skills.is_empty(),
            ".git file is a boundary too; got {:?}",
            d.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn config_skills_root_never_rescanned_as_project() {
        // cwd outside any git repo, config root on the walk-up path (~/.claude):
        // the config skills root must be scanned ONCE as User — never a second
        // time as Project. Debris is the tell: it has no path-dedup downstream.
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let config = home.join(".claude");
        let cwd = home.join("projects").join("x");
        fs::create_dir_all(&cwd).unwrap();
        write_skill(&config.join("skills"), "personal", "personal");
        fs::create_dir_all(config.join("skills").join("leftover")).unwrap();

        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd,
        });

        let personal: Vec<_> = d.skills.iter().filter(|s| s.name == "personal").collect();
        assert_eq!(personal.len(), 1);
        assert_eq!(personal[0].source, SkillSource::User);
        assert_eq!(
            d.debris.len(),
            1,
            "config skills scanned twice double-counts debris; got {:?}",
            d.debris
        );
    }

    #[test]
    fn reserved_dirs_not_discovered() {
        // plugins/marketplaces/.../skills/x/SKILL.md → NOT discovered
        // plugins/repos/o/r/skills/x/SKILL.md → NOT discovered
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        write_skill(
            &config
                .join("plugins")
                .join("marketplaces")
                .join("whatever")
                .join("skills"),
            "x",
            "x",
        );
        write_skill(
            &config
                .join("plugins")
                .join("repos")
                .join("o")
                .join("r")
                .join("skills"),
            "x",
            "x",
        );
        let d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        assert!(
            d.skills.is_empty(),
            "marketplaces/ and repos/ should not be discovered; got: {:?}",
            d.skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    // --- v0.2: project_root = the boundary dir the scope/lens hang off ---

    #[test]
    fn project_root_is_first_git_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("dev").join("repo");
        let cwd = repo.join("a").join("b");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        assert_eq!(
            project_root(&cwd, &tmp.path().join("claude")),
            Some(repo.clone())
        );
        // .git as worktree FILE is a boundary too
        let wt = tmp.path().join("wt");
        let wcwd = wt.join("sub");
        fs::create_dir_all(&wcwd).unwrap();
        fs::write(wt.join(".git"), "gitdir: /elsewhere").unwrap();
        assert_eq!(project_root(&wcwd, &tmp.path().join("claude")), Some(wt));
    }

    #[test]
    fn project_root_falls_back_to_skills_owner_without_git() {
        // No .git anywhere: the deepest ancestor owning a non-config
        // .claude/skills is still "the project".
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("proj");
        let cwd = proj.join("sub");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(proj.join(".claude").join("skills")).unwrap();
        assert_eq!(project_root(&cwd, &tmp.path().join("claude")), Some(proj));
    }

    #[test]
    fn project_root_none_outside_any_project_and_skips_config() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let config = home.join(".claude");
        let cwd = home.join("docs");
        fs::create_dir_all(&cwd).unwrap();
        // config skills exist on the walk-up path but are NOT a project root
        fs::create_dir_all(config.join("skills")).unwrap();
        assert_eq!(project_root(&cwd, &config), None);
    }

    #[test]
    fn apply_disabled_marks_only_matching_plugin_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        write_skill(&config.join("skills"), "cfo", "cfo");
        write_skill(
            &config
                .join("plugins")
                .join("cache")
                .join("o")
                .join("cart")
                .join("skills"),
            "map",
            "map",
        );
        let mut d = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        let off: std::collections::BTreeSet<String> = ["cart".to_string()].into();
        d.apply_disabled(&off);
        assert!(
            d.skills
                .iter()
                .find(|s| s.name == "cart:map")
                .unwrap()
                .disabled
        );
        assert!(!d.skills.iter().find(|s| s.name == "cfo").unwrap().disabled);
    }

    #[test]
    fn load_skill_splits_always_on_and_on_fire_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let d = config.join("skills").join("split");
        fs::create_dir_all(&d).unwrap();
        // description = 40 chars → always_on 10; body = 80 chars → est_tokens 20
        let desc = "d".repeat(40);
        let body = "b".repeat(80);
        fs::write(
            d.join("SKILL.md"),
            format!("---\nname: split\ndescription: {desc}\n---\n{body}"),
        )
        .unwrap();
        let disc = discover(&DiscoverInput {
            config_dir: config,
            cwd: tmp.path().to_path_buf(),
        });
        let s = disc.skills.iter().find(|s| s.name == "split").unwrap();
        assert_eq!(s.always_on_tokens, 10);
        assert!(s.est_tokens >= 20); // body incl. trailing newline noise ≥ 80/4
        assert!(!s.disabled);
    }
}
