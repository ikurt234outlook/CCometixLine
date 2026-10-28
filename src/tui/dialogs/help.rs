//! `?`: every key that works here.

use super::{Dialog, Reply};
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub const GLOBAL: [(&str, &str); 12] = [
    ("←→", "切换页面（标签行上）"),
    ("↑ esc", "回到标签行"),
    ("↓", "从标签行进入页面"),
    ("^S", "保存"),
    ("^Z ^Y", "撤销、重做"),
    ("[ ]", "切换预览场景"),
    ("- +", "预览宽度"),
    ("{", "预览色彩深度"),
    ("}", "预览字形"),
    ("?", "本帮助"),
    ("q", "退出"),
    ("^C", "退出"),
];

pub struct Help {
    pub page: &'static str,
    pub keys: Vec<(&'static str, &'static str)>,
}

fn rows(keys: &[(&str, &str)]) -> Vec<Line<'static>> {
    keys.iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("  {k:<10}"), ui::accent()),
                Span::raw(d.to_string()),
            ])
        })
        .collect()
}

impl Dialog for Help {
    fn render(&mut self, f: &mut Frame, area: Rect, _: &Cx) {
        let height = (GLOBAL.len() + self.keys.len() + 6) as u16;
        let rect = ui::popup(f, area, 56, height);
        let mut text = vec![Line::styled(" 全局快捷键", ui::title())];
        text.extend(rows(&GLOBAL));
        text.push(Line::raw(""));
        text.push(Line::styled(format!(" {}", self.page), ui::strong()));
        text.extend(rows(&self.keys));
        f.render_widget(
            Paragraph::new(text)
                .block(ui::panel("快捷键").title_bottom(Line::styled(" 任意键关闭 ", ui::muted()))),
            rect,
        );
    }

    fn handle(&mut self, _: KeyEvent, _: &mut Doc) -> Reply {
        Reply::Close
    }
}
