//! 6 Claude Code: the status line and agent panel settings in Claude
//! Code's `settings.json` (edited as a draft, written after a diff), the
//! update check, and what the companion mod will add.

use super::{Cx, Env, Outcome, Page, PageId};
use crate::cc_settings::{self, Settings, StatusLine};
use crate::companion::{self, Status};
use crate::state::update;
use crate::tui::dialogs::{ModSteps, SettingsDiff, shown_path};
use crate::tui::doc::Doc;
use crate::tui::ui;
use crate::version::Version;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pane {
    #[default]
    StatusLine,
    Mod,
    Agents,
    Updates,
}

/// In the order they sit on screen: left to right, top to bottom.
const PANES: [Pane; 4] = [Pane::StatusLine, Pane::Mod, Pane::Agents, Pane::Updates];
/// Rows per pane; the status line's last row applies the draft.
const ROWS: [usize; 4] = [5, 2, 2, 3];
const REFRESH: [Option<u64>; 7] = [
    None,
    Some(1),
    Some(2),
    Some(5),
    Some(10),
    Some(30),
    Some(60),
];
const INTERVALS: [u32; 5] = [6, 12, 24, 48, 168];
const UPGRADE: &str = "npm update -g @cometix/ccline";
/// Oldest Claude Code with mods.
const MODS: Version = Version {
    major: 2,
    minor: 1,
    patch: 287,
};
/// Label column per pane.
fn label_width(pane: Pane) -> usize {
    match pane {
        Pane::StatusLine => 17,
        Pane::Agents => 16,
        Pane::Updates => 13,
        Pane::Mod => 9,
    }
}

#[derive(Default)]
pub struct ClaudePage {
    pane: Pane,
    rows: [usize; 4],
    /// The file as read, or why it can't be used.
    base: Option<Result<Settings, String>>,
    /// Changes not yet written.
    draft: Option<Settings>,
}

fn step<T: PartialEq + Copy>(all: &[T], current: T, forward: bool) -> T {
    let i = all.iter().position(|v| *v == current).unwrap_or(0);
    let n = all.len();
    all[(i + if forward { 1 } else { n - 1 }) % n]
}

fn on_off(on: bool) -> Span<'static> {
    ui::choice(if on { "开" } else { "关" })
}

impl ClaudePage {
    /// Read the file again if it changed; drop a draft it now matches.
    fn sync(&mut self, path: &Path) {
        let fresh = matches!(&self.base, Some(Ok(s)) if !s.stale() && s.path == path);
        if fresh {
            return;
        }
        let loaded = Settings::load(path);
        if let (Some(d), Ok(new)) = (&self.draft, &loaded)
            && !d.differs(new)
        {
            self.draft = None;
        }
        self.base = Some(loaded);
    }

    fn base(&self) -> Option<&Settings> {
        self.base.as_ref()?.as_ref().ok()
    }

    /// The settings as edited so far.
    fn current(&self) -> Option<&Settings> {
        self.draft.as_ref().or(self.base())
    }

    fn pending(&self) -> bool {
        matches!((&self.draft, self.base()), (Some(d), Some(b)) if d.differs(b))
    }

    /// Edit the draft, starting one from the file.
    fn edit(&mut self, f: impl FnOnce(&mut Settings)) {
        if self.draft.is_none() {
            self.draft = self.base().cloned();
        }
        if let Some(d) = self.draft.as_mut() {
            f(d);
        }
    }

    /// Change the status line; there must be one first.
    fn edit_line(&mut self, f: impl FnOnce(&mut StatusLine)) -> Outcome {
        let Some(mut line) = self.current().and_then(Settings::status_line) else {
            return Outcome::Toast("请先设置 command".into());
        };
        f(&mut line);
        self.edit(|d| d.set_status_line(Some(&line)));
        Outcome::Consumed
    }

    /// The draft's changes to the file, if any.
    fn diff(&self) -> Option<SettingsDiff> {
        match (&self.draft, self.base()) {
            (Some(d), Some(b)) if d.differs(b) => Some(SettingsDiff::new(b.clone(), d.clone())),
            _ => None,
        }
    }

    fn review(&self) -> Outcome {
        match self.diff() {
            Some(diff) => Outcome::Open(Box::new(diff)),
            None => Outcome::Toast("没有要应用的更改".into()),
        }
    }

    fn block(&self, title: String, pane: Option<Pane>, cx: &Cx) -> Block<'static> {
        let b = ui::panel(&title);
        if cx.focused && pane == Some(self.pane) {
            b.border_style(ui::accent())
        } else {
            b
        }
    }

    /// A selectable row in `pane`, its value cut to fit `width`.
    fn row(
        &self,
        pane: Pane,
        i: usize,
        label: &str,
        value: Vec<Span<'static>>,
        (cx, width): (&Cx, usize),
    ) -> Line<'static> {
        let on = cx.focused && self.pane == pane && self.rows[pane as usize] == i;
        let w = label_width(pane);
        let mut spans = vec![
            Span::styled(if on { ui::SELECTED } else { "  " }, ui::accent()),
            Span::styled(
                ui::pad(label, w),
                if on { ui::accent() } else { Style::default() },
            ),
        ];
        spans.extend(ui::clip(value, width.saturating_sub(2 + w)));
        Line::from(spans)
    }

    /// A row that only shows something, aligned with `pane`'s rows.
    fn info(pane: Pane, label: &str, value: Span<'static>, width: usize) -> Line<'static> {
        let w = label_width(pane);
        let mut spans = vec![Span::raw(format!("  {}", ui::pad(label, w)))];
        spans.extend(ui::clip(vec![value], width.saturating_sub(2 + w)));
        Line::from(spans)
    }

    fn render_status_line(&self, f: &mut Frame, area: Rect, cx: &Cx) {
        let title = format!("statusLine · {}", shown_path(&cx.env.settings));
        let block = self.block(title, Some(Pane::StatusLine), cx);
        let width = block.inner(area).width as usize;
        let at = (cx, width);
        let lines = match (&self.base, self.current()) {
            (Some(Err(e)), _) => vec![
                Line::styled(format!("无法编辑：{e}"), Style::default().fg(ui::BAD)),
                Line::styled("请手动修好这个文件，再回来。", ui::muted()),
            ],
            (_, None) => vec![Line::styled("读取中…", ui::muted())],
            (_, Some(s)) => {
                let line = s.status_line();
                let room = width.saturating_sub(2 + label_width(Pane::StatusLine) + 2);
                let command = match &line {
                    Some(l) if cc_settings::is_ccline(&l.command) => {
                        let mut v = ui::clip(vec![Span::raw(l.command.clone())], room);
                        v.push(Span::styled(" ✓", Style::default().fg(ui::OK)));
                        v
                    }
                    Some(l) => vec![
                        Span::raw(crate::render::width::truncate(
                            &l.command,
                            room.saturating_sub(11),
                        )),
                        Span::styled("  非 ccline", Style::default().fg(ui::WARN)),
                    ],
                    None => vec![Span::styled("未设置", Style::default().fg(ui::WARN))],
                };
                let l = line.clone().unwrap_or_default();
                let has = line.is_some();
                let value = |v: Vec<Span<'static>>| {
                    if has {
                        v
                    } else {
                        vec![Span::styled("—", ui::muted())]
                    }
                };
                let refresh = match l.refresh_interval {
                    Some(n) => vec![ui::choice(&format!("{n}s"))],
                    None => vec![
                        ui::choice("未设置"),
                        Span::styled("  2s 可让计时器持续刷新", ui::muted()),
                    ],
                };
                let pane = Pane::StatusLine;
                let mut out = vec![
                    self.row(pane, 0, "command", command, at),
                    self.row(
                        pane,
                        1,
                        "padding",
                        value(vec![ui::choice(&l.padding.unwrap_or(0).to_string())]),
                        at,
                    ),
                    self.row(pane, 2, "refreshInterval", value(refresh), at),
                    self.row(
                        pane,
                        3,
                        "隐藏 vim 模式",
                        value(vec![on_off(l.hide_vim_mode_indicator.unwrap_or(false))]),
                        at,
                    ),
                    Line::raw(""),
                ];
                let apply = if self.pending() {
                    Span::styled("查看并应用更改…", Style::default().fg(ui::WARN))
                } else {
                    Span::styled("无更改", ui::muted())
                };
                let on = cx.focused && self.pane == pane && self.rows[0] == 4;
                out.push(Line::from(vec![
                    Span::styled(if on { ui::SELECTED } else { "  " }, ui::accent()),
                    apply,
                ]));
                if line
                    .as_ref()
                    .is_none_or(|l| !cc_settings::is_ccline(&l.command))
                {
                    out.push(Line::raw(""));
                    out.push(Line::from(ui::clip(
                        vec![
                            Span::styled("enter", ui::accent()),
                            Span::styled(" 在 command 上使用 ", ui::muted()),
                            Span::raw(cc_settings::suggested_command()),
                        ],
                        width,
                    )));
                }
                out
            }
        };
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_agents(&self, f: &mut Frame, area: Rect, cx: &Cx) {
        let pane = Pane::Agents;
        let block = self.block("Agent 面板 · subagentStatusLine".into(), Some(pane), cx);
        let width = block.inner(area).width as usize;
        let enabled = self
            .current()
            .and_then(Settings::subagent_command)
            .is_some();
        let line = cx
            .doc
            .config
            .subagent
            .clone()
            .unwrap_or_else(crate::presets::default_subagent);
        let ids = |s: &[crate::config::ItemSpec]| -> String {
            s.iter()
                .map(|i| i.config().metric.clone())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut summary = ids(&line.left);
        if !line.right.is_empty() {
            summary = format!("{summary} ⇥ {}", ids(&line.right));
        }
        let text = vec![
            self.row(pane, 0, "启用", vec![on_off(enabled)], (cx, width)),
            self.row(
                pane,
                1,
                "每个任务显示",
                vec![Span::raw(summary)],
                (cx, width),
            ),
            Line::raw(""),
            Line::styled(
                "每个后台 agent 一行，每 5 秒刷新。在上面那行回车可到「布局」页编辑（t 切换行）。",
                ui::muted(),
            ),
        ];
        f.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }).block(block),
            area,
        );
    }

    fn render_updates(&self, f: &mut Frame, area: Rect, cx: &Cx) {
        let pane = Pane::Updates;
        let block = self.block("更新".into(), Some(pane), cx);
        let width = block.inner(area).width as usize;
        let u = &cx.doc.config.update;
        let latest = match &cx.env.update.latest {
            Some(v) if update::is_newer(v, update::CURRENT) => {
                Span::styled(format!("{v}  ↑ 可更新"), Style::default().fg(ui::WARN))
            }
            Some(v) => Span::raw(format!("{v}  已是最新")),
            None => Span::styled("尚未检查", ui::muted()),
        };
        let text = vec![
            self.row(pane, 0, "检查", vec![on_off(u.check)], (cx, width)),
            self.row(
                pane,
                1,
                "每隔",
                vec![ui::choice(&format!("{}h", u.interval_hours))],
                (cx, width),
            ),
            self.row(pane, 2, "升级", vec![Span::raw(UPGRADE)], (cx, width)),
            Self::info(pane, "当前", Span::raw(update::CURRENT), width),
            Self::info(pane, "最新", latest, width),
            Line::raw(""),
            Line::styled(
                "在后台检查，不会拖慢状态栏。在「升级」行回车可复制命令。",
                ui::muted(),
            ),
        ];
        f.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }).block(block),
            area,
        );
    }

    fn render_mod(&self, f: &mut Frame, area: Rect, cx: &Cx) {
        let pane = Pane::Mod;
        let block = self.block("配套 mod".into(), Some(pane), cx);
        let width = block.inner(area).width as usize;
        let at = (cx, width);
        let ok = |t: String| Span::styled(t, Style::default().fg(ui::OK));
        let warn = |t: String| Span::styled(t, Style::default().fg(ui::WARN));
        let status = match companion::status() {
            Status::NotInstalled => vec![
                Span::styled("未安装", ui::muted()),
                Span::styled("  回车安装", ui::muted()),
            ],
            Status::Outdated(i) => vec![
                warn(format!(
                    "已装 {}，这里是 {}",
                    i.version.as_deref().unwrap_or("?"),
                    companion::version()
                )),
                Span::styled("  回车更新", ui::muted()),
            ],
            Status::Current(_) => vec![ok(format!("已安装 {} ✓", companion::version()))],
        };
        let needs = match cx.env.cc_version() {
            Some(v) if v >= MODS => ok(format!("Claude Code ≥ {MODS} ✓")),
            Some(v) => warn(format!("需要 Claude Code ≥ {MODS}，当前 {v}")),
            None => Span::raw(format!("Claude Code ≥ {MODS}")),
        };
        // Whether the mod wrote anything for the last session seen.
        let session = cx
            .env
            .session
            .as_ref()
            .and_then(|s| s.last_input.as_ref())
            .and_then(|i| i.get("session_id")?.as_str().map(str::to_string));
        let now_ms = chrono::Utc::now().timestamp_millis();
        let bridge = match session.and_then(|id| crate::state::bridge::read(&id, now_ms)) {
            Some(b) => ok(format!(
                "实时，{} 前",
                crate::metrics::fmt::duration_ms((now_ms - b.updated_at).max(0) as u64)
            )),
            None => Span::styled("上次会话没有数据", ui::muted()),
        };
        let installed = !matches!(companion::status(), Status::NotInstalled);
        let text = vec![
            self.row(pane, 0, "状态", status, at),
            self.row(
                pane,
                1,
                "移除",
                vec![Span::styled(
                    if installed { "回车移除" } else { "—" },
                    ui::muted(),
                )],
                at,
            ),
            Self::info(pane, "需要", needs, width),
            Self::info(pane, "桥接", bridge, width),
            Self::info(
                pane,
                "追加",
                Span::raw("按类型统计的上下文、压缩点、"),
                width,
            ),
            Self::info(pane, "", Span::raw("工具、agent、每轮耗时"), width),
        ];
        f.render_widget(Paragraph::new(text).block(block), area);
    }

    fn handle_mod(&mut self, key: KeyEvent) -> Outcome {
        let installed = !matches!(companion::status(), Status::NotInstalled);
        match (self.rows[Pane::Mod as usize], key.code) {
            (0, KeyCode::Enter) => Outcome::Open(Box::new(ModSteps { install: true })),
            (1, KeyCode::Enter) if installed => {
                Outcome::Open(Box::new(ModSteps { install: false }))
            }
            (1, KeyCode::Enter) => Outcome::Toast("尚未安装配套 mod".into()),
            _ => Outcome::Ignored,
        }
    }

    fn handle_status_line(&mut self, key: KeyEvent) -> Outcome {
        let forward = matches!(
            key.code,
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
        );
        match (self.rows[0], key.code) {
            (0, KeyCode::Enter) => {
                let mut line = self
                    .current()
                    .and_then(Settings::status_line)
                    .unwrap_or_default();
                line.command = cc_settings::suggested_command();
                self.edit(|d| d.set_status_line(Some(&line)));
                Outcome::Consumed
            }
            (1, KeyCode::Left | KeyCode::Right) => self.edit_line(|l| {
                let p = l.padding.unwrap_or(0);
                l.padding = Some(if forward {
                    (p + 1).min(10)
                } else {
                    p.saturating_sub(1)
                });
            }),
            (2, KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')) => {
                self.edit_line(|l| l.refresh_interval = step(&REFRESH, l.refresh_interval, forward))
            }
            (3, KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')) => self
                .edit_line(|l| {
                    let on = !l.hide_vim_mode_indicator.unwrap_or(false);
                    l.hide_vim_mode_indicator = on.then_some(true);
                }),
            (1..=3, KeyCode::Delete | KeyCode::Backspace) => {
                let row = self.rows[0];
                self.edit_line(|l| match row {
                    1 => l.padding = None,
                    2 => l.refresh_interval = None,
                    _ => l.hide_vim_mode_indicator = None,
                })
            }
            (4, KeyCode::Enter) => self.review(),
            _ => Outcome::Ignored,
        }
    }

    fn handle_agents(&mut self, key: KeyEvent) -> Outcome {
        match (self.rows[1], key.code) {
            (0, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) => {
                let on = self
                    .current()
                    .and_then(Settings::subagent_command)
                    .is_some();
                let command = self
                    .current()
                    .and_then(Settings::status_line)
                    .map(|l| l.command)
                    .filter(|c| cc_settings::is_ccline(c))
                    .unwrap_or_else(cc_settings::suggested_command);
                self.edit(|d| d.set_subagent((!on).then_some(command.as_str())));
                Outcome::Toast("已改动草稿；请在 statusLine 面板按 a 应用".into())
            }
            (1, KeyCode::Enter) => Outcome::Goto(PageId::Layout),
            _ => Outcome::Ignored,
        }
    }

    fn handle_updates(&mut self, key: KeyEvent, doc: &mut Doc) -> Outcome {
        let forward = !matches!(key.code, KeyCode::Left);
        match (self.rows[2], key.code) {
            (0, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) => {
                doc.edit(|c, _| c.update.check = !c.update.check);
                Outcome::Consumed
            }
            (1, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) => {
                doc.edit(|c, _| {
                    c.update.interval_hours = step(&INTERVALS, c.update.interval_hours, forward)
                });
                Outcome::Consumed
            }
            (2, KeyCode::Enter) => Outcome::Copy(UPGRADE.into()),
            _ => Outcome::Ignored,
        }
    }
}

impl Page for ClaudePage {
    fn render(&mut self, f: &mut Frame, area: Rect, cx: &Cx) {
        self.sync(&cx.env.settings);
        let [top, bottom] =
            Layout::vertical([Constraint::Length(10), Constraint::Fill(1)]).areas(area);
        let [status, module] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .spacing(1)
                .areas(top);
        let [agents, updates] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .spacing(1)
                .areas(bottom);
        self.render_status_line(f, status, cx);
        self.render_mod(f, module, cx);
        self.render_agents(f, agents, cx);
        self.render_updates(f, updates, cx);
    }

    fn handle(&mut self, key: KeyEvent, doc: &mut Doc, env: &Env) -> Outcome {
        self.sync(&env.settings);
        let p = self.pane as usize;
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                let n = PANES.len();
                let step = if key.code == KeyCode::Tab { 1 } else { n - 1 };
                self.pane = PANES[(p + step) % n];
                return Outcome::Consumed;
            }
            KeyCode::Up if self.rows[p] == 0 => return Outcome::Ignored,
            KeyCode::Up => {
                self.rows[p] -= 1;
                return Outcome::Consumed;
            }
            KeyCode::Down => {
                self.rows[p] = (self.rows[p] + 1).min(ROWS[p] - 1);
                return Outcome::Consumed;
            }
            KeyCode::Char('a') => return self.review(),
            _ => {}
        }
        if matches!(self.pane, Pane::StatusLine | Pane::Agents) && self.base().is_none() {
            return Outcome::Ignored;
        }
        match self.pane {
            Pane::StatusLine => self.handle_status_line(key),
            Pane::Mod => self.handle_mod(key),
            Pane::Agents => self.handle_agents(key),
            Pane::Updates => self.handle_updates(key, doc),
        }
    }

    fn keys(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("tab", "面板"),
            ("←→", "修改"),
            ("enter", "设置"),
            ("a", "应用到 settings.json"),
        ]
    }

    fn preview(&self) -> bool {
        false
    }

    fn file(&self) -> &'static str {
        "settings.json"
    }

    fn unapplied(&self) -> Option<SettingsDiff> {
        self.diff()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
    use crate::tui::app::tests::{ctrl, env, key, screen};
    use crate::tui::doc::tests::temp_paths;

    #[test]
    fn edits_a_draft_and_applies_it_after_the_diff() {
        let (dir, paths) = temp_paths("claude");
        let settings = dir.join("settings.json");
        std::fs::write(
            &settings,
            "{\n  \"statusLine\": {\"type\": \"command\", \"command\": \"bash x.sh\"}\n}\n",
        )
        .unwrap();
        let mut app = App::new(
            Doc::open(paths),
            Env {
                settings: settings.clone(),
                ..env()
            },
        );
        app.open_page(PageId::Claude);
        let s = screen(&mut app, 112, 33);
        assert!(s.contains("非 ccline") && s.contains("配套 mod"), "{s}");

        // Point the command at ccline, set a refresh interval.
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
        let s = screen(&mut app, 112, 33);
        assert!(s.contains("查看并应用") && s.contains("‹2s›"), "{s}");
        assert!(
            std::fs::read_to_string(&settings)
                .unwrap()
                .contains("bash x.sh")
        );

        // The diff, then the write and its backup.
        app.handle_key(key(KeyCode::Char('a')));
        let s = screen(&mut app, 112, 33);
        assert!(s.contains("+   \"refreshInterval\": 2"), "{s}");
        app.handle_key(key(KeyCode::Char('y')));
        let written = Settings::load(&settings).unwrap();
        let line = written.status_line().unwrap();
        assert!(cc_settings::is_ccline(&line.command));
        assert_eq!(line.refresh_interval, Some(2));
        assert!(dir.join("settings.json.ccline-bak").exists());
        assert!(screen(&mut app, 112, 33).contains("无更改"));

        // The companion mod pane shows the commands before running any.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Enter));
        let s = screen(&mut app, 112, 33);
        assert!(
            s.contains("安装配套 mod") && s.contains("cometix@ccline"),
            "{s}"
        );
        app.handle_key(key(KeyCode::Char('n')));

        // The agent panel uses the same command.
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('y')));
        let written = Settings::load(&settings).unwrap();
        assert_eq!(written.subagent_command(), Some(line.command));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_or_quitting_brings_up_a_drafted_change() {
        let (dir, paths) = temp_paths("claude-save");
        let settings = dir.join("settings.json");
        std::fs::write(
            &settings,
            "{\n  \"statusLine\": {\"type\": \"command\", \"command\": \"ccline\"}\n}\n",
        )
        .unwrap();
        let mut app = App::new(
            Doc::open(paths),
            Env {
                settings: settings.clone(),
                ..env()
            },
        );
        app.open_page(PageId::Claude);
        screen(&mut app, 112, 33);
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Right));
        let refresh = || {
            Settings::load(&settings)
                .unwrap()
                .status_line()
                .unwrap()
                .refresh_interval
        };

        // ^S shows the diff; N leaves the draft as it was.
        app.handle_key(ctrl('s'));
        assert!(screen(&mut app, 112, 33).contains("+   \"refreshInterval\": 2"));
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(refresh(), None);

        // Quitting asks first; saving on the way out applies it, then quits.
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.quit);
        assert!(screen(&mut app, 112, 33).contains("settings.json 的改动尚未应用"));
        app.handle_key(key(KeyCode::Char('s')));
        assert!(screen(&mut app, 112, 33).contains("应用并退出"));
        app.handle_key(key(KeyCode::Char('y')));
        assert!(app.quit);
        assert_eq!(refresh(), Some(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
