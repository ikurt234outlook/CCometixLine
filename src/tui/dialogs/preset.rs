//! K · The user's presets: saving the current setup as one, renaming one,
//! and the prompts before replacing or deleting one. Each writes its file
//! at once; only marking the config as the preset is an undoable edit.

use super::form::{Field, Form, FormKey};
use super::{Action, Dialog, Reply, shown_path};
use crate::presets;
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

const WIDTH: u16 = 72;

/// `path` as shown, cut at the start to `width` columns so the file name
/// stays in view.
fn path_in(path: &std::path::Path, width: usize) -> String {
    let shown = shown_path(path);
    let chars: Vec<char> = shown.chars().collect();
    if chars.len() <= width {
        return shown;
    }
    let keep = &chars[chars.len() - width.saturating_sub(1)..];
    format!("…{}", keep.iter().collect::<String>())
}

/// N saves the current layout and look as a new preset; E renames one of
/// the user's and edits its summary.
pub struct PresetDialog {
    /// The preset being renamed, or `None` for a new one.
    editing: Option<String>,
    form: Form,
    error: Option<String>,
}

impl PresetDialog {
    pub fn new() -> Self {
        Self::build(None, "", "")
    }

    pub fn edit(name: &str, summary: &str) -> Self {
        Self::build(Some(name.to_string()), name, summary)
    }

    fn build(editing: Option<String>, name: &str, summary: &str) -> Self {
        PresetDialog {
            editing,
            form: Form::new(
                vec![
                    Field::text("名称", name, "例如 work"),
                    Field::text("简介", summary, "可选，显示在画廊中"),
                ],
                0,
            ),
            error: None,
        }
    }

    /// The name as it will be saved.
    fn name(&self) -> String {
        presets::slug(&self.form.fields[0].input.text())
    }

    /// Why the typed name can't be used, if it can't.
    fn problem(&self, doc: &Doc) -> Option<String> {
        let name = self.name();
        if self.editing.as_deref() == Some(name.as_str()) {
            return None;
        }
        presets::name_problem(&name).or_else(|| {
            doc.presets
                .get(&name)
                .filter(|e| e.is_mine())
                .map(|_| format!("你已经有一个 {name} 了。"))
        })
    }
}

impl Dialog for PresetDialog {
    fn paste(&mut self, text: &str) -> bool {
        self.error = None;
        self.form.paste(text)
    }

    fn render(&mut self, f: &mut Frame, area: Rect, cx: &Cx) {
        let mut text = self.form.lines(WIDTH as usize - 2, &[]);
        text.push(Line::raw(""));
        let typed = !self.form.fields[0].input.text().trim().is_empty();
        let bad = Style::default().fg(ui::BAD);
        text.push(match (&self.error, self.problem(cx.doc)) {
            (Some(e), _) => Line::styled(format!(" {e}"), bad),
            (None, Some(p)) if typed => Line::styled(format!(" {p}"), bad),
            (None, Some(_)) => Line::styled(" 命名后即可保存。", ui::muted()),
            (None, None) => {
                let path = cx.doc.presets.dir().join(format!("{}.toml", self.name()));
                let room = WIDTH as usize - 10;
                Line::styled(format!(" 保存到 {}", path_in(&path, room)), ui::muted())
            }
        });
        if self.editing.is_none() {
            text.push(Line::styled(
                " 只保存布局与外观；宽度、git 与更新设置不会写入。",
                ui::muted(),
            ));
        }
        text.push(Line::raw(""));
        text.push(ui::hints(&[
            ("enter", "保存"),
            ("esc", "取消"),
            ("tab", "下一字段"),
        ]));
        let rect = ui::popup(f, area, WIDTH, text.len() as u16 + 2);
        let title = match &self.editing {
            Some(name) => format!("编辑预设 {name}"),
            None => "另存为预设".into(),
        };
        f.render_widget(Paragraph::new(text).block(ui::panel(&title)), rect);
    }

    fn handle(&mut self, key: KeyEvent, doc: &mut Doc) -> Reply {
        match self.form.handle(key) {
            FormKey::Cancel => Reply::Close,
            FormKey::Submit => {
                let name = self.name();
                let summary = self.form.fields[1].value();
                let done = match &self.editing {
                    None => doc
                        .presets
                        .create(&name, &summary, &doc.config)
                        .map(|()| format!("已保存预设 {name}")),
                    Some(old) => doc.presets.rename(old, &name, &summary).map(|()| {
                        if *old == name {
                            format!("已更新 {name}")
                        } else {
                            format!("已把 {old} 重命名为 {name}")
                        }
                    }),
                };
                match done {
                    Err(e) => {
                        self.error = Some(e);
                        Reply::Stay
                    }
                    Ok(message) => {
                        // The config now is this preset (a new one), or
                        // follows it to its new name.
                        let mark = match &self.editing {
                            None => true,
                            Some(old) => doc.config.preset.as_deref() == Some(old.as_str()),
                        };
                        if mark {
                            doc.edit(|c, _| c.preset = Some(name));
                        }
                        Reply::Then(Action::Toast(message))
                    }
                }
            }
            _ => {
                self.error = None;
                Reply::Stay
            }
        }
    }
}

/// Y/N before replacing one of the user's presets with the current setup,
/// or deleting it.
pub struct PresetPrompt {
    name: String,
    delete: bool,
}

impl PresetPrompt {
    pub fn replace(name: String) -> Self {
        PresetPrompt {
            name,
            delete: false,
        }
    }

    pub fn delete(name: String) -> Self {
        PresetPrompt { name, delete: true }
    }
}

impl Dialog for PresetPrompt {
    fn render(&mut self, f: &mut Frame, area: Rect, cx: &Cx) {
        let name = &self.name;
        let mut text = if self.delete {
            let path = cx.doc.presets.dir().join(format!("{name}.toml"));
            let mut t = vec![
                Line::raw(format!(" 删除 {name}？这将移除")),
                Line::raw(format!("   {}", path_in(&path, WIDTH as usize - 3))),
            ];
            if cx.doc.config.preset.as_deref() == Some(name.as_str()) {
                t.push(Line::styled(
                    " 你的状态栏外观不变；只是不再标记为此预设。",
                    ui::muted(),
                ));
            }
            t
        } else {
            vec![
                Line::raw(format!(" 用当前设置替换 {name}？")),
                Line::styled(" 它会采用预览当前显示的布局与外观。", ui::muted()),
            ]
        };
        text.push(Line::raw(""));
        text.push(ui::hints(&[
            ("Y", if self.delete { "删除" } else { "替换" }),
            ("N", "取消"),
        ]));
        let rect = ui::popup(f, area, WIDTH + 4, text.len() as u16 + 2);
        let title = if self.delete {
            "删除预设"
        } else {
            "替换预设"
        };
        f.render_widget(Paragraph::new(text).block(ui::panel(title)), rect);
    }

    fn handle(&mut self, key: KeyEvent, doc: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                let name = self.name.clone();
                let done = if self.delete {
                    doc.presets.delete(&name).map(|()| format!("已删除 {name}"))
                } else {
                    doc.presets
                        .replace(&name, &doc.config)
                        .map(|()| format!("已用你的设置替换 {name}"))
                };
                Reply::Then(match done {
                    Ok(message) => {
                        if !self.delete {
                            doc.edit(|c, _| c.preset = Some(name));
                        }
                        Action::Toast(message)
                    }
                    Err(e) => Action::Error(e),
                })
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Reply::Close,
            _ => Reply::Stay,
        }
    }
}
