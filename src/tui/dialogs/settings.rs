//! F · Changing Claude Code's `settings.json`: the change as a diff, then
//! a backup and the write.

use super::{Action, Dialog, Reply};
use crate::cc_settings::{Mark, Settings};
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const WIDTH: u16 = 72;
/// Most diff lines shown.
const MAX_LINES: usize = 16;

pub struct SettingsDiff {
    /// The file as read.
    pub base: Settings,
    /// The settings to write.
    pub draft: Settings,
    /// Quit once they're written: the last step of saving on the way out.
    pub then_quit: bool,
}

impl SettingsDiff {
    pub fn new(base: Settings, draft: Settings) -> SettingsDiff {
        SettingsDiff {
            base,
            draft,
            then_quit: false,
        }
    }
}

/// `path` with the home directory as `~`.
pub fn shown_path(path: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|h| path.strip_prefix(h).ok().map(|p| p.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl Dialog for SettingsDiff {
    fn render(&mut self, f: &mut Frame, area: Rect, _: &Cx) {
        let diff = self.draft.diff(&self.base);
        let shown = diff.len().min(MAX_LINES);
        let more = u16::from(diff.len() > shown);
        let rect = ui::popup(f, area, WIDTH, shown as u16 + more + 7);
        let room = WIDTH as usize - 2;
        let mut text = vec![
            Line::raw(" ccline 将把以下内容写入你的 Claude Code 设置："),
            Line::raw(""),
        ];
        for (mark, line) in diff.iter().take(shown) {
            let (sign, style) = match mark {
                Mark::Same => ("  ", Style::default()),
                Mark::Added => ("+ ", Style::default().fg(ui::OK)),
                Mark::Removed => ("- ", Style::default().fg(ui::BAD)),
            };
            text.push(Line::from(ui::clip(
                vec![Span::styled(format!("  {sign}{line}"), style)],
                room,
            )));
        }
        if diff.len() > shown {
            text.push(Line::styled(
                format!("    … 另有 {} 行", diff.len() - shown),
                ui::muted(),
            ));
        }
        text.push(Line::raw(""));
        text.push(Line::from(vec![
            Span::styled(" 备份  ", ui::muted()),
            Span::raw(format!(
                "{}.ccline-bak",
                self.base
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )),
        ]));
        let apply = if self.then_quit {
            "应用并退出"
        } else {
            "应用"
        };
        text.push(ui::hints(&[("Y", apply), ("N", "取消")]));
        let title = format!("更改 {}", shown_path(&self.base.path));
        f.render_widget(Paragraph::new(text).block(ui::panel(&title)), rect);
    }

    fn handle(&mut self, key: KeyEvent, _: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Char('y' | 'Y') => match self.draft.save(&self.base) {
                Ok(_) if self.then_quit => Reply::Then(Action::Quit),
                Ok(backup) => Reply::Then(Action::Toast(match backup {
                    Some(b) => format!("已更新 settings.json · 旧文件在 {}", shown_path(&b)),
                    None => "已创建 settings.json".into(),
                })),
                Err(e) => Reply::Then(Action::Error(format!("无法写入 settings.json：{e}"))),
            },
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Reply::Close,
            _ => Reply::Stay,
        }
    }
}
