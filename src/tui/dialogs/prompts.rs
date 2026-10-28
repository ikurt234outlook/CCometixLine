//! Small yes/no prompts: quitting with unsaved edits, and saving over a
//! file that changed on disk.

use super::{Action, Dialog, Reply};
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use std::path::PathBuf;

/// I · Unsaved changes.
pub struct Quit {
    /// A change to Claude Code's settings is drafted and not written.
    pub settings: bool,
}

impl Dialog for Quit {
    fn render(&mut self, f: &mut Frame, area: Rect, cx: &Cx) {
        let n = cx.doc.edits();
        let mut text = Vec::new();
        if n > 0 {
            text.push(Line::raw(format!(" 上次保存后有 {n} 处改动。")));
        }
        if self.settings {
            text.push(Line::raw(" 有一处对 settings.json 的改动尚未应用。"));
        }
        text.push(Line::raw(""));
        text.push(ui::hints(&[
            ("S", "保存并退出"),
            ("D", "放弃"),
            ("esc", "留下"),
        ]));
        let rect = ui::popup(f, area, 64, text.len() as u16 + 4);
        f.render_widget(Paragraph::new(text).block(ui::panel("未保存的改动")), rect);
    }

    fn handle(&mut self, key: KeyEvent, _: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Char('s' | 'S') => Reply::Then(Action::Save {
                force: false,
                then_quit: true,
            }),
            KeyCode::Char('d' | 'D') => Reply::Then(Action::Quit),
            KeyCode::Esc => Reply::Close,
            _ => Reply::Stay,
        }
    }
}

/// A file changed on disk after it was loaded.
pub struct Conflict {
    pub paths: Vec<PathBuf>,
    pub then_quit: bool,
}

impl Dialog for Conflict {
    fn render(&mut self, f: &mut Frame, area: Rect, _: &Cx) {
        let rect = ui::popup(f, area, 72, 8 + self.paths.len() as u16);
        let mut text = vec![Line::raw(" 打开后已被 ccline 之外改动的文件：")];
        text.extend(
            self.paths
                .iter()
                .map(|p| Line::styled(format!("   {}", p.display()), ui::accent())),
        );
        text.push(Line::raw(""));
        text.push(Line::raw(
            " 覆盖会丢失这些改动；重新加载会丢失你自己的改动。",
        ));
        text.push(Line::raw(""));
        text.push(ui::hints(&[
            ("O", "覆盖"),
            ("R", "重新加载"),
            ("esc", "取消"),
        ]));
        f.render_widget(Paragraph::new(text).block(ui::panel("磁盘上已改动")), rect);
    }

    fn handle(&mut self, key: KeyEvent, _: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Char('o' | 'O') => Reply::Then(Action::Save {
                force: true,
                then_quit: self.then_quit,
            }),
            KeyCode::Char('r' | 'R') => Reply::Then(Action::Reload),
            KeyCode::Esc => Reply::Close,
            _ => Reply::Stay,
        }
    }
}
