//! G · The command palette (^P): every action, found by typing a few
//! letters of it.

use super::{Action, Dialog, Reply};
use crate::config::{Config, ItemSpec, LineConfig};
use crate::metrics;
use crate::presets::{self, Library, Parts};
use crate::scenarios::SCENARIOS;
use crate::style::palette;
use crate::style::{GlyphTier, SkinKind};
use crate::tui::doc::Doc;
use crate::tui::pages::{Cx, PageId};
use crate::tui::ui::{self, TextInput};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const WIDTH: u16 = 70;
/// Matches shown at once.
const SHOWN: usize = 10;

type Edit = Box<dyn Fn(&mut Config)>;

enum Run {
    App(Action),
    Edit(Edit),
}

struct Command {
    group: &'static str,
    label: String,
    run: Run,
}

fn app(group: &'static str, label: impl Into<String>, action: Action) -> Command {
    Command {
        group,
        label: label.into(),
        run: Run::App(action),
    }
}

fn edit(
    group: &'static str,
    label: impl Into<String>,
    f: impl Fn(&mut Config) + 'static,
) -> Command {
    Command {
        group,
        label: label.into(),
        run: Run::Edit(Box::new(f)),
    }
}

fn lower<T: std::fmt::Debug>(v: T) -> String {
    format!("{v:?}").to_lowercase()
}

fn all(presets: &Library) -> Vec<Command> {
    let mut out = Vec::new();
    for p in PageId::ALL {
        out.push(app("跳转", format!("前往 {}", p.title()), Action::Goto(p)));
    }
    out.push(app(
        "文件",
        "保存",
        Action::Save {
            force: false,
            then_quit: false,
        },
    ));
    out.push(app("文件", "撤销", Action::Undo));
    out.push(app("文件", "重做", Action::Redo));
    out.push(app("文件", "从磁盘重新加载", Action::Reload));
    out.push(app("文件", "退出", Action::AskQuit));
    out.push(app("引导", "运行首次启动设置", Action::Setup));
    out.push(app("帮助", "本页快捷键", Action::Help));
    for entry in presets.entries() {
        let Ok(preset) = entry.config.clone() else {
            continue;
        };
        out.push(edit("预设", format!("应用 {}", entry.name), move |c| {
            *c = presets::apply(c, &preset, Parts::All);
        }));
    }
    out.push(app("预设", "把当前设置另存为新预设", Action::SavePreset));
    for name in palette::available() {
        let label = name.clone();
        out.push(edit("配色", label, move |c| c.palette = name.clone()));
    }
    for skin in [
        SkinKind::Plain,
        SkinKind::Bracket,
        SkinKind::Pill,
        SkinKind::Powerline,
    ] {
        out.push(edit("皮肤", lower(skin), move |c| c.skin = skin));
    }
    for tier in [GlyphTier::Nerd, GlyphTier::Unicode, GlyphTier::Ascii] {
        out.push(edit("字形", lower(tier), move |c| c.glyphs = tier));
    }
    for m in metrics::catalog() {
        let id = m.meta.id;
        if id.starts_with("task.") {
            continue;
        }
        // Like the library: on the lines once, or not at all.
        out.push(edit("布局", format!("添加或移除 {id}"), move |c| {
            let had = c
                .lines
                .iter()
                .flat_map(|l| l.left.iter().chain(&l.right))
                .any(|s| s.config().metric == id);
            if had && id != "cmd" {
                for line in &mut c.lines {
                    line.left.retain(|s| s.config().metric != id);
                    line.right.retain(|s| s.config().metric != id);
                }
                return;
            }
            if c.lines.is_empty() {
                c.lines.push(LineConfig::default());
            }
            if let Some(line) = c.lines.last_mut() {
                line.left.push(ItemSpec::Id(id.to_string()));
            }
        }));
    }
    for (i, s) in SCENARIOS.iter().enumerate() {
        out.push(app("预览", format!("场景 {}", s.name), Action::Scenario(i)));
    }
    out.push(app("预览", "重放上次会话", Action::Replay));
    out
}

/// Whether `query`'s letters appear in `text` in order, and how well:
/// lower is better, `None` for no match.
fn score(query: &str, text: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    if let Some(at) = text.find(query) {
        return Some(at);
    }
    let mut chars = text.char_indices();
    let mut first = None;
    let mut last = 0;
    for q in query.chars() {
        let (i, _) = chars.find(|(_, c)| *c == q)?;
        first.get_or_insert(i);
        last = i;
    }
    // Spread-out matches rank after tight ones and after substrings.
    Some(100 + last - first.unwrap_or(0))
}

pub struct Commands {
    input: TextInput,
    row: usize,
    all: Vec<Command>,
}

impl Commands {
    pub fn new(presets: &Library) -> Self {
        Commands {
            input: TextInput::default(),
            row: 0,
            all: all(presets),
        }
    }

    fn matches(&self) -> Vec<&Command> {
        let q = self.input.text().trim().to_lowercase();
        let mut found: Vec<(usize, &Command)> = self
            .all
            .iter()
            .filter_map(|c| {
                let text = format!("{} {}", c.group, c.label).to_lowercase();
                Some((score(&q, &text)?, c))
            })
            .collect();
        found.sort_by_key(|(s, _)| *s);
        found.into_iter().map(|(_, c)| c).collect()
    }
}

impl Dialog for Commands {
    fn render(&mut self, f: &mut Frame, area: Rect, _: &Cx) {
        let count = self.matches().len();
        self.row = self.row.min(count.saturating_sub(1));
        let matches = self.matches();
        let start = self.row.saturating_sub(SHOWN - 1);
        let rect = ui::popup(f, area, WIDTH, SHOWN as u16 + 6);
        let mut text = vec![
            Line::from(
                [
                    vec![Span::styled(" > ", ui::accent())],
                    self.input
                        .line(WIDTH as usize - 6, true, "输入以搜索")
                        .spans,
                ]
                .concat(),
            ),
            Line::raw(""),
        ];
        for (i, c) in matches.iter().enumerate().skip(start).take(SHOWN) {
            let on = i == self.row;
            text.push(Line::from(vec![
                Span::styled(if on { ui::SELECTED } else { "  " }, ui::accent()),
                Span::styled(format!("{:<9}", c.group), ui::muted()),
                Span::styled(
                    c.label.clone(),
                    if on { ui::accent() } else { Style::default() },
                ),
            ]));
        }
        if matches.is_empty() {
            text.push(Line::styled("  没有匹配项", ui::muted()));
        }
        while text.len() < SHOWN + 2 {
            text.push(Line::raw(""));
        }
        text.push(Line::raw(""));
        text.push(ui::hints(&[
            ("↑↓", "选择"),
            ("enter", "运行"),
            ("esc", "关闭"),
        ]));
        f.render_widget(Paragraph::new(text).block(ui::panel("命令")), rect);
    }

    fn handle(&mut self, key: KeyEvent, doc: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Esc => return Reply::Close,
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => self.row += 1,
            KeyCode::Enter => {
                let Some(c) = self.matches().into_iter().nth(self.row) else {
                    return Reply::Stay;
                };
                return match &c.run {
                    Run::App(action) => Reply::Then(action.clone()),
                    Run::Edit(f) => {
                        doc.edit(|config, _| f(config));
                        Reply::Then(Action::Toast(format!("{} {}", c.group, c.label)))
                    }
                };
            }
            _ => {
                if self.input.handle(key) {
                    self.row = 0;
                }
            }
        }
        Reply::Stay
    }

    fn paste(&mut self, text: &str) -> bool {
        self.input.insert_str(text.trim());
        self.row = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::doc::tests::temp_paths;
    use ratatui::crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn finds_and_runs_commands() {
        assert_eq!(score("pill", "skin pill"), Some(5));
        assert!(score("sknp", "skin pill").is_some_and(|s| s >= 100));
        assert_eq!(score("zz", "skin pill"), None);

        let (dir, paths) = temp_paths("commands");
        let mut doc = Doc::open(paths);
        let mut c = Commands::new(&doc.presets);
        for ch in "皮肤 pill".chars() {
            c.handle(key(KeyCode::Char(ch)), &mut doc);
        }
        assert_eq!(c.matches()[0].label, "pill");
        c.handle(key(KeyCode::Enter), &mut doc);
        assert_eq!(doc.config.skin, SkinKind::Pill);

        let mut c = Commands::new(&doc.presets);
        for ch in "跳转 布局".chars() {
            c.handle(key(KeyCode::Char(ch)), &mut doc);
        }
        assert!(matches!(
            c.handle(key(KeyCode::Enter), &mut doc),
            Reply::Then(Action::Goto(PageId::Layout))
        ));

        let run = |doc: &mut Doc| {
            let mut c = Commands::new(&doc.presets);
            for ch in "移除 cost.today".chars() {
                c.handle(key(KeyCode::Char(ch)), doc);
            }
            c.handle(key(KeyCode::Enter), doc);
        };
        run(&mut doc);
        let last = doc.config.lines.last().unwrap();
        assert_eq!(last.left.last(), Some(&ItemSpec::Id("cost.today".into())));
        // Run again, it comes off: never twice.
        run(&mut doc);
        let all: Vec<_> = doc
            .config
            .lines
            .iter()
            .flat_map(|l| l.left.iter())
            .collect();
        assert!(!all.contains(&&ItemSpec::Id("cost.today".into())));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
