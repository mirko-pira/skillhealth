use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct ParsedSkill {
    pub name: Option<String>,
    pub description: Option<String>,
    pub body: String,
    pub frontmatter_ok: bool,
}

/// Strip surrounding single or double quotes from a YAML scalar value.
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Lenient line-based extraction for frontmatter that fails strict YAML parsing
/// (e.g. unquoted colon-space in description values).
/// Returns `(name, description)` — at least one must be `Some` for the block to
/// be considered valid.
fn lenient_extract(yaml: &str) -> (Option<String>, Option<String>) {
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if name.is_none()
            && let Some(rest) = trimmed.strip_prefix("name:")
        {
            let val = strip_quotes(rest.trim());
            if !val.is_empty() {
                name = Some(val.to_string());
            }
        }
        if description.is_none()
            && let Some(rest) = trimmed.strip_prefix("description:")
        {
            let val = strip_quotes(rest.trim());
            if !val.is_empty() {
                description = Some(val.to_string());
            }
        }
        if name.is_some() && description.is_some() {
            break;
        }
    }

    (name, description)
}

pub fn parse_skill_md(content: &str) -> ParsedSkill {
    let not_ok = |body: &str| ParsedSkill {
        name: None,
        description: None,
        body: body.to_string(),
        frontmatter_ok: false,
    };

    let Some(rest) = content.strip_prefix("---") else {
        return not_ok(content);
    };
    let Some(end) = rest.find("\n---") else {
        return not_ok(content);
    };
    let yaml = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n').to_string();
    if yaml.trim().is_empty() {
        return ParsedSkill { body, ..not_ok("") };
    }
    match serde_norway::from_str::<Frontmatter>(yaml) {
        Ok(fm) => ParsedSkill {
            name: fm.name,
            description: fm.description,
            body,
            frontmatter_ok: true,
        },
        Err(_) => {
            // Strict YAML failed — try lenient line-based extraction.
            // This handles the common case of an unquoted colon-space in description
            // (e.g. `description: Audit deps: flag anything unpinned`), which Claude Code's
            // own loader accepts but serde_norway rejects.
            let (name, description) = lenient_extract(yaml);
            if name.is_some() || description.is_some() {
                ParsedSkill {
                    name,
                    description,
                    body,
                    frontmatter_ok: true,
                }
            } else {
                ParsedSkill { body, ..not_ok("") }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_frontmatter() {
        let md = "---\nname: cfo\ndescription: Personal CFO advisor\n---\n\n# CFO\nBody here.";
        let p = parse_skill_md(md);
        assert!(p.frontmatter_ok);
        assert_eq!(p.name.as_deref(), Some("cfo"));
        assert_eq!(p.description.as_deref(), Some("Personal CFO advisor"));
        assert!(p.body.starts_with("# CFO"));
    }

    #[test]
    fn missing_frontmatter_is_not_ok() {
        let p = parse_skill_md("# Just a title\nNo frontmatter.");
        assert!(!p.frontmatter_ok);
        assert_eq!(p.name, None);
        assert_eq!(p.body, "# Just a title\nNo frontmatter.");
    }

    #[test]
    fn unterminated_frontmatter_is_not_ok() {
        let p = parse_skill_md("---\nname: broken\nno closing fence");
        assert!(!p.frontmatter_ok);
    }

    #[test]
    fn invalid_yaml_is_not_ok_but_body_preserved() {
        let p = parse_skill_md("---\n: [ : :\n---\nBody");
        assert!(!p.frontmatter_ok);
        assert_eq!(p.body, "Body");
    }

    #[test]
    fn extra_frontmatter_keys_are_ignored() {
        let md = "---\nname: x\ndescription: d\nallowed-tools: [Bash]\n---\nB";
        let p = parse_skill_md(md);
        assert!(p.frontmatter_ok);
    }

    #[test]
    fn empty_frontmatter_block_is_not_ok() {
        let p = parse_skill_md("---\n---\nBody");
        assert!(!p.frontmatter_ok);
    }

    // Fix 1 regression tests: colon-in-description must not produce false E001.

    #[test]
    fn colon_in_description_is_ok_via_lenient_fallback() {
        let md = "---\nname: dep-audit\ndescription: Audit deps: flag anything unpinned and outdated\n---\nBody";
        let p = parse_skill_md(md);
        assert!(
            p.frontmatter_ok,
            "colon-in-description should not produce E001"
        );
        assert_eq!(p.name.as_deref(), Some("dep-audit"));
        assert_eq!(
            p.description.as_deref(),
            Some("Audit deps: flag anything unpinned and outdated")
        );
        assert_eq!(p.body, "Body");
    }

    #[test]
    fn colon_in_description_with_only_description_is_ok() {
        // Even without a name: line the fallback should accept based on description alone.
        let md = "---\ndescription: release-notes: notes for the next tag\n---\nBody";
        let p = parse_skill_md(md);
        assert!(p.frontmatter_ok);
        assert_eq!(
            p.description.as_deref(),
            Some("release-notes: notes for the next tag")
        );
        assert_eq!(p.name, None);
    }

    #[test]
    fn quoted_colon_description_parsed_by_strict_yaml() {
        // serde_norway handles quoted values natively; lenient path should NOT be hit.
        let md =
            "---\nname: perf-audit\ndescription: \"Perf audit: profile the hot paths\"\n---\nBody";
        let p = parse_skill_md(md);
        assert!(p.frontmatter_ok);
        assert_eq!(
            p.description.as_deref(),
            Some("Perf audit: profile the hot paths")
        );
    }

    #[test]
    fn truly_invalid_yaml_with_no_keys_remains_not_ok() {
        // The existing `---\n: [ : :\n---\nBody` case: no name:/description: lines
        // → lenient fallback finds nothing → frontmatter_ok must stay false.
        let p = parse_skill_md("---\n: [ : :\n---\nBody");
        assert!(!p.frontmatter_ok);
        assert_eq!(p.body, "Body");
    }
}
