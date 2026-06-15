use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const HOT_DAYS: i64 = 7;
pub const WARM_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Temperature {
    Hot,
    Warm,
    Cold,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Error,
}

pub fn temperature(last_used: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Temperature {
    match last_used {
        None => Temperature::Dead,
        Some(t) if now - t <= Duration::days(HOT_DAYS) => Temperature::Hot,
        Some(t) if now - t <= Duration::days(WARM_DAYS) => Temperature::Warm,
        Some(_) => Temperature::Cold,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "plugin")]
pub enum SkillSource {
    User,
    Project,
    Plugin(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    /// Display name: dir name, or "plugin:dir" for plugin skills.
    pub name: String,
    pub dir_name: String,
    pub source: SkillSource,
    /// Path to SKILL.md.
    pub path: PathBuf,
    pub description: Option<String>,
    pub frontmatter_ok: bool,
    /// Markdown body (frontmatter stripped). Used for graph; not serialized.
    #[serde(skip)]
    pub body: String,
    /// chars/4 heuristic.
    pub est_tokens: usize,
    /// Description chars/4 — the always-on cost, paid in the system prompt
    /// every session. `est_tokens` is the on-fire (body) side.
    pub always_on_tokens: usize,
    /// Plugin switched off via enabledPlugins: installed but never loaded —
    /// neither dead nor cold.
    pub disabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageStats {
    pub count: u64,
    pub last_used: Option<DateTime<Utc>>,
    /// Mentions per absolute ISO week (key "2026-W24"). Absolute keys stay
    /// valid in the per-file cache across time; the report computes the
    /// relative 12-week window from these at aggregation time.
    #[serde(default)]
    pub week_counts: BTreeMap<String, u32>,
}

/// "2026-W24" — ISO week year + zero-padded ISO week number.
pub fn iso_week_key(t: DateTime<Utc>) -> String {
    let w = t.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()
    }

    #[test]
    fn never_used_is_dead() {
        assert_eq!(temperature(None, now()), Temperature::Dead);
    }

    #[test]
    fn used_today_is_hot() {
        let t = Utc.with_ymd_and_hms(2026, 6, 10, 9, 0, 0).unwrap();
        assert_eq!(temperature(Some(t), now()), Temperature::Hot);
    }

    #[test]
    fn used_exactly_7_days_ago_is_hot() {
        let t = Utc.with_ymd_and_hms(2026, 6, 3, 12, 0, 0).unwrap();
        assert_eq!(temperature(Some(t), now()), Temperature::Hot);
    }

    #[test]
    fn used_10_days_ago_is_warm() {
        let t = Utc.with_ymd_and_hms(2026, 5, 31, 12, 0, 0).unwrap();
        assert_eq!(temperature(Some(t), now()), Temperature::Warm);
    }

    #[test]
    fn used_45_days_ago_is_cold() {
        let t = Utc.with_ymd_and_hms(2026, 4, 26, 12, 0, 0).unwrap();
        assert_eq!(temperature(Some(t), now()), Temperature::Cold);
    }

    #[test]
    fn iso_week_key_formats_year_and_zero_padded_week() {
        let t = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
        assert_eq!(iso_week_key(t), "2026-W24");
    }

    #[test]
    fn iso_week_key_uses_iso_year_at_boundary() {
        // Mon 2025-12-29 belongs to ISO week 1 of 2026.
        let t = Utc.with_ymd_and_hms(2025, 12, 29, 0, 0, 0).unwrap();
        assert_eq!(iso_week_key(t), "2026-W01");
    }
}
