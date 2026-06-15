use crossterm::event::{KeyCode, KeyEvent};
use skillhealth_core::doctor::Finding;
use skillhealth_core::model::{Severity, SkillSource, Temperature};
use skillhealth_core::report::{Report, ReportSkill};
use skillhealth_core::view::{Lens, Scope};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Usage,
    Name,
    Temperature,
    Tokens,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::Usage => SortMode::Name,
            SortMode::Name => SortMode::Temperature,
            SortMode::Temperature => SortMode::Tokens,
            SortMode::Tokens => SortMode::Usage,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Usage => "usage",
            SortMode::Name => "name",
            SortMode::Temperature => "temp",
            SortMode::Tokens => "tokens",
        }
    }
}

/// Side effects the event loop must perform. The state machine never does I/O.
#[derive(Debug)]
pub enum Action {
    Quit,
    Refresh,
    OpenEditor(PathBuf),
    OpenGraph,
    CopyFix(String),
    SetView(Scope, Lens),
}

pub struct App {
    pub view: View,
    pub report: Option<Report>,
    /// Selection tracked BY NAME so refreshes never steal focus.
    pub selected: Option<String>,
    pub doctor_idx: usize,
    pub filter: String,
    pub filter_input: bool,
    pub sort: SortMode,
    pub group_by_source: bool,
    pub show_help: bool,
    pub scanning: bool,
    /// Watcher running — header shows "live" vs "live off".
    pub live: bool,
    /// Header flash countdown, decremented on tick after a refresh.
    pub flash_frames: u8,
    pub toast: Option<String>,
    /// Animation frame counter (spinner, pulse), bumped on tick.
    pub tick: u64,
    /// Active scope — synced from the last report and mutated by `p`.
    pub scope: Scope,
    /// Active lens — synced from the last report and mutated by `L`.
    pub lens: Lens,
    /// Whether a project root was detected (gates the Project stop in the `p` cycle).
    pub project_available: bool,
    /// Human-readable label for the project root (shown in header).
    pub project_label: Option<String>,
}

fn temp_rank(t: Temperature) -> u8 {
    match t {
        Temperature::Hot => 0,
        Temperature::Warm => 1,
        Temperature::Cold => 2,
        Temperature::Dead => 3,
    }
}

fn source_rank(s: &SkillSource) -> u8 {
    match s {
        SkillSource::User => 0,
        SkillSource::Project => 1,
        SkillSource::Plugin(_) => 2,
    }
}

impl App {
    pub fn new() -> Self {
        App {
            view: View::Overview,
            report: None,
            selected: None,
            doctor_idx: 0,
            filter: String::new(),
            filter_input: false,
            sort: SortMode::Usage,
            group_by_source: false,
            show_help: false,
            scanning: false,
            live: false,
            flash_frames: 0,
            toast: None,
            tick: 0,
            scope: Scope::All,
            lens: Lens::Global,
            project_available: false,
            project_label: None,
        }
    }

    /// Skills after filter + sort + optional grouping — the list as displayed.
    pub fn visible(&self) -> Vec<&ReportSkill> {
        let Some(report) = &self.report else {
            return Vec::new();
        };
        let needle = self.filter.to_lowercase();
        let mut skills: Vec<&ReportSkill> = report
            .skills
            .iter()
            .filter(|s| needle.is_empty() || s.name.to_lowercase().contains(&needle))
            .collect();
        skills.sort_by(|a, b| match self.sort {
            SortMode::Usage => b
                .usage
                .count
                .cmp(&a.usage.count)
                .then_with(|| a.name.cmp(&b.name)),
            SortMode::Name => a.name.cmp(&b.name),
            SortMode::Temperature => temp_rank(a.temperature)
                .cmp(&temp_rank(b.temperature))
                .then_with(|| b.usage.count.cmp(&a.usage.count))
                .then_with(|| a.name.cmp(&b.name)),
            SortMode::Tokens => b
                .est_tokens
                .cmp(&a.est_tokens)
                .then_with(|| a.name.cmp(&b.name)),
        });
        skills.sort_by_key(|s| s.disabled);
        if self.group_by_source {
            skills.sort_by_key(|s| source_rank(&s.source)); // stable: keeps sort within groups
        }
        skills
    }

    pub fn visible_names(&self) -> Vec<String> {
        self.visible().iter().map(|s| s.name.clone()).collect()
    }

    pub fn selected_skill(&self) -> Option<&ReportSkill> {
        let name = self.selected.as_deref()?;
        self.visible().into_iter().find(|s| s.name == name)
    }

    /// Doctor findings in display order: errors first, then warnings.
    pub fn sorted_findings(&self) -> Vec<&Finding> {
        let Some(report) = &self.report else {
            return Vec::new();
        };
        let mut f: Vec<&Finding> = report.findings.iter().collect();
        f.sort_by_key(|f| match f.severity {
            Severity::Error => 0,
            Severity::Warn => 1,
        });
        f
    }

    pub fn apply_report(&mut self, report: Report) {
        self.scope = report.view.scope;
        self.lens = report.view.lens;
        self.project_available = report.view.project_root.is_some();
        self.project_label = report.view.project_label.clone();
        self.report = Some(report);
        let names = self.visible_names();
        self.selected = match self.selected.take() {
            Some(name) if names.contains(&name) => Some(name),
            _ => names.first().cloned(),
        };
        let findings = self.sorted_findings().len();
        if self.doctor_idx >= findings {
            self.doctor_idx = findings.saturating_sub(1);
        }
        self.scanning = false;
        self.flash_frames = 6;
    }

    fn move_selection(&mut self, delta: i64) {
        let names = self.visible_names();
        if names.is_empty() {
            self.selected = None;
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|n| names.iter().position(|x| x == n))
            .unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, names.len() as i64 - 1) as usize;
        self.selected = Some(names[next].clone());
    }

    /// Returns the side effect for the event loop, if any.
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.show_help {
            self.show_help = false;
            return None;
        }
        if self.filter_input {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filter_input = false;
                }
                KeyCode::Enter => self.filter_input = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.move_selection(0); // re-clamp selection into filtered list
                }
                _ => {}
            }
            self.move_selection(0);
            return None;
        }
        match (self.view, key.code) {
            (_, KeyCode::Char('q')) => return Some(Action::Quit),
            (_, KeyCode::Esc) => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.move_selection(0);
                } else {
                    return Some(Action::Quit);
                }
            }
            (_, KeyCode::Char('?')) => self.show_help = true,
            (_, KeyCode::Char('r')) => return Some(Action::Refresh),
            (_, KeyCode::Char('g')) => return Some(Action::OpenGraph),
            (_, KeyCode::Char('p')) => {
                self.scope = self.scope.next(self.project_available);
                self.lens = self.scope.coupled_lens();
                return Some(Action::SetView(self.scope, self.lens));
            }
            (_, KeyCode::Char('L')) => {
                self.lens = self.lens.toggled();
                return Some(Action::SetView(self.scope, self.lens));
            }
            (_, KeyCode::Char('d') | KeyCode::Tab) => {
                self.view = match self.view {
                    View::Overview => View::Doctor,
                    View::Doctor => View::Overview,
                };
            }
            (View::Overview, KeyCode::Char('j') | KeyCode::Down) => self.move_selection(1),
            (View::Overview, KeyCode::Char('k') | KeyCode::Up) => self.move_selection(-1),
            (View::Overview, KeyCode::Char('/')) => self.filter_input = true,
            (View::Overview, KeyCode::Char('s')) => self.sort = self.sort.next(),
            (View::Overview, KeyCode::Char('S')) => self.group_by_source = !self.group_by_source,
            (View::Overview, KeyCode::Enter | KeyCode::Char('o')) => {
                if let Some(s) = self.selected_skill() {
                    return Some(Action::OpenEditor(s.path.clone()));
                }
            }
            (View::Doctor, KeyCode::Char('j') | KeyCode::Down) => {
                let len = self.sorted_findings().len();
                if len > 0 {
                    self.doctor_idx = (self.doctor_idx + 1).min(len - 1);
                }
            }
            (View::Doctor, KeyCode::Char('k') | KeyCode::Up) => {
                self.doctor_idx = self.doctor_idx.saturating_sub(1);
            }
            (View::Doctor, KeyCode::Enter) => {
                if let Some(name) = self
                    .sorted_findings()
                    .get(self.doctor_idx)
                    .and_then(|f| f.skill.clone())
                {
                    self.view = View::Overview;
                    self.filter.clear();
                    self.selected = Some(name);
                }
            }
            (View::Doctor, KeyCode::Char('y')) => {
                if let Some(fix) = self
                    .sorted_findings()
                    .get(self.doctor_idx)
                    .and_then(|f| f.fix.clone())
                {
                    return Some(Action::CopyFix(fix));
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::fixtures::fixture_report;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app_with_report() -> App {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app
    }

    #[test]
    fn apply_report_selects_first_visible_and_stops_scanning() {
        let app = app_with_report();
        // default sort = usage desc → cfo (9 uses) first
        assert_eq!(app.selected.as_deref(), Some("cfo"));
        assert!(!app.scanning);
        assert!(app.flash_frames > 0);
    }

    #[test]
    fn selection_is_preserved_by_name_across_refresh() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('j'))); // → finance
        assert_eq!(app.selected.as_deref(), Some("finance"));
        app.apply_report(fixture_report()); // refresh with same data
        assert_eq!(app.selected.as_deref(), Some("finance"));
    }

    #[test]
    fn selection_falls_back_when_skill_disappears() {
        let mut app = app_with_report();
        app.selected = Some("ghost".into());
        app.apply_report(fixture_report());
        assert_eq!(app.selected.as_deref(), Some("cfo"));
    }

    #[test]
    fn jk_and_arrows_move_and_clamp() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('k'))); // already first → stays
        assert_eq!(app.selected.as_deref(), Some("cfo"));
        for _ in 0..10 {
            app.on_key(key(KeyCode::Down));
        }
        // clamped at last visible (usage sort: old-experiment has 0 uses)
        assert_eq!(app.selected.as_deref(), Some("old-experiment"));
    }

    #[test]
    fn filter_narrows_case_insensitive_and_esc_clears_before_quit() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('/')));
        assert!(app.filter_input);
        for c in "FIN".chars() {
            app.on_key(key(KeyCode::Char(c.to_ascii_lowercase())));
        }
        app.on_key(key(KeyCode::Enter)); // confirm, leave input mode
        assert!(!app.filter_input);
        assert_eq!(app.visible_names(), vec!["finance".to_string()]);
        // Esc clears the filter instead of quitting
        let action = app.on_key(key(KeyCode::Esc));
        assert!(action.is_none());
        assert!(app.filter.is_empty());
        // second Esc quits
        assert!(matches!(app.on_key(key(KeyCode::Esc)), Some(Action::Quit)));
    }

    #[test]
    fn sort_cycles_usage_name_temperature_tokens() {
        let mut app = app_with_report();
        assert_eq!(app.sort, SortMode::Usage);
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!(app.sort, SortMode::Name);
        assert_eq!(app.visible_names()[0], "cfo"); // alphabetical
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!(app.sort, SortMode::Temperature);
        assert_eq!(app.visible_names()[0], "cfo"); // hot first
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!(app.sort, SortMode::Tokens);
        assert_eq!(app.visible_names()[0], "superpowers:writing-plans"); // 4000 tokens
        app.on_key(key(KeyCode::Char('s')));
        assert_eq!(app.sort, SortMode::Usage);
    }

    #[test]
    fn group_by_source_orders_user_project_plugin() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('S')));
        assert!(app.group_by_source);
        let names = app.visible_names();
        // user (cfo, old-experiment by usage) → project (finance) → plugin
        assert_eq!(
            names,
            vec![
                "cfo".to_string(),
                "old-experiment".to_string(),
                "finance".to_string(),
                "superpowers:writing-plans".to_string()
            ]
        );
    }

    #[test]
    fn tab_and_d_toggle_doctor_view() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('d')));
        assert_eq!(app.view, View::Doctor);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.view, View::Overview);
    }

    #[test]
    fn doctor_enter_jumps_to_affected_skill_in_overview() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('d')));
        // doctor_idx 0 = the E001 finding about old-experiment
        let action = app.on_key(key(KeyCode::Enter));
        assert!(action.is_none());
        assert_eq!(app.view, View::Overview);
        assert_eq!(app.selected.as_deref(), Some("old-experiment"));
    }

    #[test]
    fn doctor_y_returns_copy_action_with_fix_text() {
        let mut app = app_with_report();
        app.on_key(key(KeyCode::Char('d')));
        match app.on_key(key(KeyCode::Char('y'))) {
            Some(Action::CopyFix(fix)) => assert!(fix.contains("old-experiment")),
            other => panic!("expected CopyFix, got {other:?}"),
        }
    }

    #[test]
    fn overview_enter_and_o_open_editor_on_selected_path() {
        let mut app = app_with_report();
        match app.on_key(key(KeyCode::Enter)) {
            Some(Action::OpenEditor(p)) => assert!(p.ends_with("cfo/SKILL.md")),
            other => panic!("expected OpenEditor, got {other:?}"),
        }
        match app.on_key(key(KeyCode::Char('o'))) {
            Some(Action::OpenEditor(_)) => {}
            other => panic!("expected OpenEditor, got {other:?}"),
        }
    }

    #[test]
    fn r_refreshes_g_opens_graph_q_quits_question_mark_helps() {
        let mut app = app_with_report();
        assert!(matches!(
            app.on_key(key(KeyCode::Char('r'))),
            Some(Action::Refresh)
        ));
        assert!(matches!(
            app.on_key(key(KeyCode::Char('g'))),
            Some(Action::OpenGraph)
        ));
        app.on_key(key(KeyCode::Char('?')));
        assert!(app.show_help);
        app.on_key(key(KeyCode::Esc)); // closes help, does not quit
        assert!(!app.show_help);
        assert!(matches!(
            app.on_key(key(KeyCode::Char('q'))),
            Some(Action::Quit)
        ));
    }

    #[test]
    fn p_cycles_scope_and_drags_lens_skipping_project_when_unavailable() {
        use skillhealth_core::view::{Lens, Scope};
        let mut app = App::new();
        app.apply_report(fixture_report()); // fixture view: All/Global, no project
        assert_eq!(app.scope, Scope::All);
        assert!(!app.project_available);
        let action = app.on_key(key(KeyCode::Char('p')));
        assert_eq!(app.scope, Scope::User);
        assert_eq!(app.lens, Lens::Global);
        assert!(matches!(
            action,
            Some(Action::SetView(Scope::User, Lens::Global))
        ));
        // user → (no project) → all
        app.on_key(key(KeyCode::Char('p')));
        assert_eq!(app.scope, Scope::All);
        // with a project available, user → project couples the lens
        app.project_available = true;
        app.scope = Scope::User;
        let action = app.on_key(key(KeyCode::Char('p')));
        assert_eq!(app.scope, Scope::Project);
        assert_eq!(app.lens, Lens::Project);
        assert!(matches!(
            action,
            Some(Action::SetView(Scope::Project, Lens::Project))
        ));
    }

    #[test]
    fn shift_l_toggles_lens_alone() {
        use skillhealth_core::view::{Lens, Scope};
        let mut app = App::new();
        app.apply_report(fixture_report());
        let action = app.on_key(key(KeyCode::Char('L')));
        assert_eq!(app.lens, Lens::Project);
        assert_eq!(app.scope, Scope::All); // scope untouched
        assert!(matches!(
            action,
            Some(Action::SetView(Scope::All, Lens::Project))
        ));
    }

    #[test]
    fn apply_report_syncs_view_state() {
        use skillhealth_core::view::{Lens, Scope};
        let mut app = App::new();
        let mut r = fixture_report();
        r.view.scope = Scope::Project;
        r.view.lens = Lens::Project;
        r.view.project_root = Some(std::path::PathBuf::from("/dev/demo-app"));
        r.view.project_label = Some("demo-app".into());
        app.apply_report(r);
        assert_eq!(app.scope, Scope::Project);
        assert_eq!(app.lens, Lens::Project);
        assert!(app.project_available);
        assert_eq!(app.project_label.as_deref(), Some("demo-app"));
    }

    #[test]
    fn disabled_skills_sort_last_within_visible() {
        let mut report = fixture_report();
        report.skills[0].disabled = true; // cfo: highest usage but disabled
        let mut app = App::new();
        app.apply_report(report);
        let names = app.visible_names();
        assert_eq!(names.last().map(String::as_str), Some("cfo"));
    }

    // Generation guard invariant test.
    //
    // The event loop holds a `scan_gen: u64` counter and tags each spawned scan
    // with it.  Only the event whose generation equals `scan_gen` is applied;
    // older ones are silently dropped.  This test simulates that policy in
    // isolation, verifying that:
    //  - a stale report (gen < current) leaves `scanning` true and the previous
    //    report intact;
    //  - the matching report (gen == current) is applied normally and clears
    //    `scanning`.
    #[test]
    fn generation_guard_stale_report_does_not_win() {
        use skillhealth_core::view::{Lens, Scope};

        // Helper that mimics what the event handler does:
        //   if gen == scan_gen { app.apply_report(*r) }
        fn maybe_apply(app: &mut App, report: Report, event_gen: u64, scan_gen: u64) {
            if event_gen == scan_gen {
                app.apply_report(report);
            }
        }

        let mut app = App::new();
        app.scanning = true;
        let current_gen: u64 = 2; // two SetView presses happened before any scan returned

        // Gen 0 report (initial scan, now stale) arrives first — must be dropped.
        let stale = fixture_report();
        maybe_apply(&mut app, stale, 0, current_gen);
        assert!(app.scanning, "stale report must not clear scanning");
        assert!(app.report.is_none(), "stale report must not be applied");

        // Gen 1 report (first SetView scan, also stale) arrives — must be dropped.
        let also_stale = fixture_report();
        maybe_apply(&mut app, also_stale, 1, current_gen);
        assert!(
            app.scanning,
            "intermediate stale report must not clear scanning"
        );
        assert!(app.report.is_none());

        // Gen 2 report (latest scan) arrives — must be applied.
        let mut fresh = fixture_report();
        fresh.view.scope = Scope::User;
        fresh.view.lens = Lens::Global;
        maybe_apply(&mut app, fresh, 2, current_gen);
        assert!(!app.scanning, "current-gen report must clear scanning");
        assert_eq!(
            app.scope,
            Scope::User,
            "current-gen report must update scope"
        );
    }
}
