use crate::model::Skill;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    SkillMention,
    ClaudeMdMention,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || c == ':'
}

fn find_word(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_word_char(haystack[..abs].chars().next_back().unwrap());
        let after = abs + needle.len();
        let after_ok =
            after >= haystack.len() || !is_word_char(haystack[after..].chars().next().unwrap());
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle.len();
    }
    false
}

pub fn mentions(haystack: &str, name: &str) -> bool {
    if find_word(haystack, &format!("/{name}")) {
        return true;
    }
    if haystack.contains(&format!("[[{name}]]")) || haystack.contains(&format!("`{name}`")) {
        return true;
    }
    if name.contains('-') || name.contains(':') {
        return find_word(haystack, name);
    }
    false
}

/// Returns the unique lookup names for a skill (name + dir_name if different).
fn unique_names(skill: &Skill) -> Vec<&str> {
    if skill.name != skill.dir_name {
        vec![skill.name.as_str(), skill.dir_name.as_str()]
    } else {
        vec![skill.name.as_str()]
    }
}

/// Build a lookup map: pattern → list of skill indices.
///
/// We index four pattern forms:
///  - `/<name>`     (slash-command, needs word-boundary after)
///  - `[[<name>]]`  (wikilink, exact)
///  - `` `<name>` ``(backtick, exact)
///  - `<name>`      (bare word, only for names with `-` or `:` to avoid false positives)
fn build_pattern_index(skills: &[Skill]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, skill) in skills.iter().enumerate() {
        for name in unique_names(skill) {
            map.entry(format!("/{name}")).or_default().push(i);
            map.entry(format!("[[{name}]]")).or_default().push(i);
            map.entry(format!("`{name}`")).or_default().push(i);
            if name.contains('-') || name.contains(':') {
                map.entry(name.to_string()).or_default().push(i);
            }
        }
    }
    map
}

/// Scan haystack for all indexed patterns and return the set of matched skill indices.
///
/// Rather than iterating over all patterns (O(patterns × body)), we extract "candidate
/// tokens" from the body in a single O(body) pass and look them up in the index.
///
/// Candidate extraction:
///  - `/word`   — slash-command tokens
///  - `[[…]]`  — wikilink tokens
///  - `` `…` `` — backtick tokens
///  - hyphenated/colon words — bare-word tokens
///
/// After lookup, word-boundary validation is applied only to slash-form and bare-word
/// tokens (O(token-length), negligible).
fn find_mentioned_skills(
    haystack: &str,
    pattern_index: &HashMap<String, Vec<usize>>,
) -> Vec<usize> {
    let mut mentioned = std::collections::HashSet::new();

    // --- 1. wikilinks: [[...]] ---
    let mut search = haystack;
    while let Some(start) = search.find("[[") {
        search = &search[start + 2..];
        if let Some(end) = search.find("]]") {
            let token = format!("[[{}]]", &search[..end]);
            if let Some(indices) = pattern_index.get(&token) {
                for &i in indices {
                    mentioned.insert(i);
                }
            }
            search = &search[end + 2..];
        }
    }

    // --- 2. backtick spans: `...` ---
    let mut search = haystack;
    while let Some(start) = search.find('`') {
        search = &search[start + 1..];
        if let Some(end) = search.find('`') {
            let token = format!("`{}`", &search[..end]);
            if let Some(indices) = pattern_index.get(&token) {
                for &i in indices {
                    mentioned.insert(i);
                }
            }
            search = &search[end + 1..];
        }
    }

    // --- 3. slash-commands and bare hyphenated/colon words ---
    // Use char_indices to safely iterate over a potentially non-ASCII string.
    let chars: Vec<(usize, char)> = haystack.char_indices().collect();
    let n = chars.len();
    let mut ci = 0;
    while ci < n {
        let (byte_pos, ch) = chars[ci];
        if ch == '/' {
            // Slash token: read the word-chars after '/'
            let start_ci = ci + 1;
            let mut end_ci = start_ci;
            while end_ci < n && is_word_char(chars[end_ci].1) {
                end_ci += 1;
            }
            if end_ci > start_ci {
                let start_byte = chars[start_ci].0;
                let end_byte = if end_ci < n {
                    chars[end_ci].0
                } else {
                    haystack.len()
                };
                let word = &haystack[start_byte..end_byte];
                let slash_token = format!("/{word}");
                if let Some(indices) = pattern_index.get(&slash_token) {
                    // Verify no word-char immediately before '/'
                    let before_ok = ci == 0 || !is_word_char(chars[ci - 1].1);
                    if before_ok {
                        for &idx in indices {
                            mentioned.insert(idx);
                        }
                    }
                }
            }
            ci += 1;
        } else if is_word_char(ch) {
            // Read a full word token
            let start_ci = ci;
            let mut end_ci = ci;
            let mut has_hyphen_or_colon = false;
            while end_ci < n && is_word_char(chars[end_ci].1) {
                if chars[end_ci].1 == '-' || chars[end_ci].1 == ':' {
                    has_hyphen_or_colon = true;
                }
                end_ci += 1;
            }
            if has_hyphen_or_colon {
                let start_byte = chars[start_ci].0;
                let end_byte = if end_ci < n {
                    chars[end_ci].0
                } else {
                    haystack.len()
                };
                let token = &haystack[start_byte..end_byte];
                if let Some(indices) = pattern_index.get(token) {
                    // Word boundary: nothing immediately before or after
                    let before_ok = start_ci == 0 || !is_word_char(chars[start_ci - 1].1);
                    let after_ok = end_ci >= n || !is_word_char(chars[end_ci].1);
                    if before_ok && after_ok {
                        for &idx in indices {
                            mentioned.insert(idx);
                        }
                    }
                }
            }
            ci = end_ci;
        } else {
            ci += 1;
        }
        let _ = byte_pos; // suppress unused variable warning
    }

    mentioned.into_iter().collect()
}

pub fn build_graph(skills: &[Skill], claude_mds: &[(String, String)]) -> Vec<Edge> {
    let pattern_index = build_pattern_index(skills);

    let mut edges = Vec::new();
    for a in skills {
        let mentioned = find_mentioned_skills(&a.body, &pattern_index);
        for b_idx in mentioned {
            let b = &skills[b_idx];
            if a.name == b.name {
                continue;
            }
            edges.push(Edge {
                from: a.name.clone(),
                to: b.name.clone(),
                kind: EdgeKind::SkillMention,
            });
        }
    }
    for (label, content) in claude_mds {
        let mentioned = find_mentioned_skills(content, &pattern_index);
        for b_idx in mentioned {
            let s = &skills[b_idx];
            edges.push(Edge {
                from: label.clone(),
                to: s.name.clone(),
                kind: EdgeKind::ClaudeMdMention,
            });
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Skill, SkillSource};
    use std::path::PathBuf;

    fn skill(name: &str, body: &str) -> Skill {
        Skill {
            name: name.to_string(),
            dir_name: name.rsplit(':').next().unwrap().to_string(),
            source: SkillSource::User,
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            description: Some("d".into()),
            frontmatter_ok: true,
            body: body.to_string(),
            est_tokens: body.len() / 4,
            always_on_tokens: 10,
            disabled: false,
        }
    }

    #[test]
    fn slash_wikilink_and_backtick_mentions_match() {
        assert!(mentions("run /cfo now", "cfo"));
        assert!(mentions("see [[cfo]] for details", "cfo"));
        assert!(mentions("use `cfo` mode", "cfo"));
    }

    #[test]
    fn common_word_bare_mentions_do_not_match() {
        assert!(!mentions("please review the code", "review"));
        assert!(!mentions("the state of things", "state"));
    }

    #[test]
    fn multiword_bare_mentions_match() {
        assert!(mentions(
            "similar to release-notes in spirit",
            "release-notes"
        ));
        assert!(!mentions(
            "release-notes-extended is different",
            "release-notes"
        ));
    }

    #[test]
    fn path_segments_do_not_match_slash_form() {
        assert!(!mentions("docs/plans is a directory", "plans"));
    }

    #[test]
    fn builds_skill_and_claude_md_edges() {
        let skills = vec![
            skill(
                "build-fix",
                "Defers to /dep-audit before retrying the build.",
            ),
            skill("dep-audit", "Dependency scanner."),
            skill("perf-audit", "Standalone."),
        ];
        let claude = vec![(
            "CLAUDE.md".to_string(),
            "Use /perf-audit before tagging a release.".to_string(),
        )];
        let edges = build_graph(&skills, &claude);
        assert!(edges.contains(&Edge {
            from: "build-fix".into(),
            to: "dep-audit".into(),
            kind: EdgeKind::SkillMention
        }));
        assert!(edges.contains(&Edge {
            from: "CLAUDE.md".into(),
            to: "perf-audit".into(),
            kind: EdgeKind::ClaudeMdMention
        }));
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn plugin_skills_match_by_unqualified_dir_name() {
        let mut wp = skill("superpowers:writing-plans", "Body.");
        wp.source = SkillSource::Plugin("superpowers".into());
        let skills = vec![skill("handoff", "After this, run /writing-plans."), wp];
        let edges = build_graph(&skills, &[]);
        assert!(edges.contains(&Edge {
            from: "handoff".into(),
            to: "superpowers:writing-plans".into(),
            kind: EdgeKind::SkillMention
        }));
    }
}
