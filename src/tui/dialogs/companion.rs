//! Installing or removing the companion mod: the `claude plugin` commands
//! that will run, then running them.

use super::{Action, Dialog, Reply};
use crate::companion;
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const WIDTH: u16 = 76;

pub struct ModSteps {
    pub install: bool,
}

impl ModSteps {
    fn steps(&self) -> Vec<Vec<String>> {
        if self.install {
            companion::install_plan()
        } else {
            companion::uninstall_steps()
        }
    }
}

impl Dialog for ModSteps {
    fn render(&mut self, f: &mut Frame, area: Rect, _: &Cx) {
        let steps = self.steps();
        let mut text = Vec::new();
        if self.install {
            text.push(Line::raw(format!(
                " ccline 会把 mod 写入 {}，然后执行：",
                crate::tui::dialogs::shown_path(&companion::dir())
            )));
        } else {
            text.push(Line::raw(" ccline 将执行："));
        }
        text.push(Line::raw(""));
        for s in &steps {
            text.push(Line::from(ui::clip(
                vec![
                    Span::styled("   $ ", ui::muted()),
                    Span::raw(companion::shown(s)),
                ],
                WIDTH as usize - 3,
            )));
        }
        text.push(Line::raw(""));
        text.push(Line::styled(
            if self.install {
                " Claude Code 会把该插件记录进它的设置。之后请重启它。"
            } else {
                " Claude Code 会从设置里移除该插件及其市场（marketplace）。"
            },
            ui::muted(),
        ));
        text.push(ui::hints(&[("Y", "执行"), ("N", "取消")]));
        let rect = ui::popup(f, area, WIDTH, text.len() as u16 + 2);
        let title = if self.install {
            "安装配套 mod"
        } else {
            "移除配套 mod"
        };
        f.render_widget(Paragraph::new(text).block(ui::panel(title)), rect);
    }

    fn handle(&mut self, key: KeyEvent, _: &mut Doc) -> Reply {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                let result = if self.install {
                    companion::install()
                } else {
                    companion::uninstall()
                };
                Reply::Then(match result {
                    Ok(()) if self.install => {
                        Action::Toast("已安装配套 mod；重启 Claude Code 即可加载".into())
                    }
                    Ok(()) => Action::Toast("已移除配套 mod".into()),
                    Err(e) => Action::Error(e),
                })
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Reply::Close,
            _ => Reply::Stay,
        }
    }
}
