//! H · First-run setup: what's there, the font, a preset, and pointing
//! Claude Code at ccline. Finishing saves the config.

use super::{Action, Dialog, Reply};
use crate::cc_settings::{self, Mark, Settings};
use crate::config::LoadNote;
use crate::presets::{self, PRESETS, Parts};
use crate::style::glyphs::{self, GlyphTier, IconKey};
use crate::tui::doc::Doc;
use crate::tui::pages::Cx;
use crate::tui::ui;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::PathBuf;

const WIDTH: u16 = 76;
const STEPS: [&str; 4] = ["检测", "字体", "预设", "连接"];

pub struct Setup {
    step: usize,
    preset: usize,
    /// Claude Code's settings file.
    settings: PathBuf,
}

impl Setup {
    pub fn new(settings: PathBuf) -> Self {
        Setup {
            step: 0,
            preset: PRESETS
                .iter()
                .position(|p| p.name == presets::DEFAULT)
                .unwrap_or(0),
            settings,
        }
    }

    fn progress(&self) -> Line<'static> {
        let mut spans = vec![Span::raw(" ")];
        for (i, name) in STEPS.iter().enumerate() {
            let (dot, style) = if i <= self.step {
                ("●", ui::accent())
            } else {
                ("○", ui::muted())
            };
            spans.push(Span::styled(format!("{dot} {name}   "), style));
        }
        Line::from(spans)
    }

    /// The settings with `statusLine` pointing at ccline, if that changes
    /// anything.
    fn connection(&self) -> Result<Option<(Settings, Settings)>, String> {
        let base = Settings::load(&self.settings)?;
        let mut line = base.status_line().unwrap_or_default();
        if cc_settings::is_ccline(&line.command) {
            return Ok(None);
        }
        line.command = cc_settings::suggested_command();
        let mut draft = base.clone();
        draft.set_status_line(Some(&line));
        Ok(Some((base, draft)))
    }

    fn detect(&self, cx: &Cx) -> Vec<Line<'static>> {
        let row = |label: &str, value: String| {
            Line::from(vec![
                Span::styled(format!("  {}", ui::pad(label, 14)), ui::muted()),
                Span::raw(value),
            ])
        };
        let cc = match cx.env.cc_version() {
            Some(v) => format!("{v}，来自你上次的会话"),
            None => "还没见到；首次使用没关系".into(),
        };
        let status = match Settings::load(&self.settings).map(|s| s.status_line()) {
            Ok(Some(l)) if cc_settings::is_ccline(&l.command) => "已在运行 ccline".to_string(),
            Ok(Some(l)) => format!("运行 {}", crate::render::width::truncate(&l.command, 40)),
            Ok(None) => "未设置".into(),
            Err(e) => e,
        };
        let config = match &cx.doc.note {
            LoadNote::Missing => "还没有".to_string(),
            LoadNote::Invalid(e) => format!("无法读取（{e}）；保存时会替换"),
            LoadNote::Fine => "你的 config.toml".into(),
        };
        let depth = format!("{:?}", crate::env::color_depth()).to_lowercase();
        vec![
            Line::raw(" ccline 检测到："),
            Line::raw(""),
            row("Claude Code", cc),
            row("statusLine", status),
            row("配置", config),
            row("颜色", depth),
        ]
    }

    fn fonts(&self) -> Vec<Line<'static>> {
        let sample = |tier: GlyphTier| -> String {
            [
                IconKey::Model,
                IconKey::Dir,
                IconKey::Git,
                IconKey::Context,
                IconKey::Cost,
            ]
            .iter()
            .map(|k| glyphs::icon(*k, tier))
            .collect::<Vec<_>>()
            .join("  ")
        };
        vec![
            Line::raw(" 这些是否显示为图标，而不是方框或问号？"),
            Line::raw(""),
            Line::from(vec![
                Span::styled("   Nerd      ", ui::muted()),
                Span::styled(
                    format!("{}    \u{e0b6}\u{e0b4} \u{e0b0}", sample(GlyphTier::Nerd)),
                    ui::strong(),
                ),
            ]),
            Line::from(vec![
                Span::styled("   Unicode   ", ui::muted()),
                Span::styled(sample(GlyphTier::Unicode), ui::strong()),
            ]),
        ]
    }

    fn presets(&self, cx: &Cx) -> Vec<Line<'static>> {
        let registry = cx.doc.registry();
        let mut lines = vec![
            Line::raw(" 挑一个起点；之后所有内容都能改。"),
            Line::raw(""),
        ];
        for (i, p) in PRESETS.iter().enumerate() {
            let on = i == self.preset;
            lines.push(Line::from(ui::clip(
                vec![
                    Span::styled(if on { ui::SELECTED } else { "  " }, ui::accent()),
                    Span::styled(
                        format!("{:<14}", p.name),
                        if on { ui::accent() } else { Style::default() },
                    ),
                    Span::styled(p.summary.to_string(), ui::muted()),
                ],
                WIDTH as usize - 3,
            )));
        }
        lines.push(Line::raw(""));
        if let Some(config) = presets::get(PRESETS[self.preset].name) {
            let mut config = config;
            config.glyphs = cx.doc.config.glyphs;
            let width = WIDTH - 4;
            for l in cx
                .preview
                .render(&config, cx.doc, &registry, cx.env.tz, width)
                .into_iter()
                .take(3)
            {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(l.spans);
                lines.push(Line::from(spans));
            }
        }
        lines
    }

    fn connect(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        match self.connection() {
            Err(e) => {
                lines.push(Line::styled(
                    format!(" 无法修改 settings.json：{e}"),
                    Style::default().fg(ui::BAD),
                ));
            }
            Ok(None) => {
                lines.push(Line::styled(
                    " Claude Code 已经在运行 ccline。",
                    Style::default().fg(ui::OK),
                ));
            }
            Ok(Some((base, draft))) => {
                lines.push(Line::raw(" 让 Claude Code 的状态栏指向 ccline："));
                lines.push(Line::raw(""));
                for (mark, line) in draft.diff(&base).into_iter().take(10) {
                    let (sign, style) = match mark {
                        Mark::Same => ("  ", Style::default()),
                        Mark::Added => ("+ ", Style::default().fg(ui::OK)),
                        Mark::Removed => ("- ", Style::default().fg(ui::BAD)),
                    };
                    lines.push(Line::from(ui::clip(
                        vec![Span::styled(format!("  {sign}{line}"), style)],
                        WIDTH as usize - 2,
                    )));
                }
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    if base.exists() {
                        " 旧文件会保留为 settings.json.ccline-bak。"
                    } else {
                        " 还没有 settings.json；这一步会创建它。"
                    },
                    ui::muted(),
                ));
            }
        }
        lines
    }

    fn hints(&self) -> Line<'static> {
        match self.step {
            0 => ui::hints(&[("enter", "下一步"), ("esc", "跳过设置")]),
            1 => ui::hints(&[
                ("N", "Nerd 字体"),
                ("U", "仅 Unicode"),
                ("A", "仅 ASCII"),
                ("esc", "跳过设置"),
            ]),
            2 => ui::hints(&[("↑↓", "选择"), ("enter", "使用"), ("esc", "跳过设置")]),
            _ => match self.connection() {
                Ok(Some(_)) => ui::hints(&[("Y", "应用并完成"), ("S", "跳过"), ("esc", "关闭")]),
                _ => ui::hints(&[("enter", "完成"), ("esc", "关闭")]),
            },
        }
    }

    /// Save the config and close.
    fn finish(&self) -> Reply {
        Reply::Then(Action::Save {
            force: false,
            then_quit: false,
        })
    }
}

impl Dialog for Setup {
    fn render(&mut self, f: &mut Frame, area: Rect, cx: &Cx) {
        let body = match self.step {
            0 => self.detect(cx),
            1 => self.fonts(),
            2 => self.presets(cx),
            _ => self.connect(),
        };
        let height = body.len() as u16 + 6;
        let rect = ui::popup(f, area, WIDTH, height);
        let mut text = vec![self.progress(), Line::raw("")];
        text.extend(body);
        text.push(Line::raw(""));
        text.push(self.hints());
        let title = format!(
            "设置 · 第 {} / {} 步 · {}",
            self.step + 1,
            STEPS.len(),
            STEPS[self.step].to_lowercase()
        );
        f.render_widget(Paragraph::new(text).block(ui::panel(&title)), rect);
    }

    fn handle(&mut self, key: KeyEvent, doc: &mut Doc) -> Reply {
        if key.code == KeyCode::Esc {
            return Reply::Close;
        }
        match (self.step, key.code) {
            (0, KeyCode::Enter) => self.step = 1,
            (1, KeyCode::Char(c)) => {
                let tier = match c.to_ascii_lowercase() {
                    'n' => GlyphTier::Nerd,
                    'u' => GlyphTier::Unicode,
                    'a' => GlyphTier::Ascii,
                    _ => return Reply::Stay,
                };
                doc.edit(|c, _| c.glyphs = tier);
                self.step = 2;
            }
            (2, KeyCode::Up) => self.preset = self.preset.saturating_sub(1),
            (2, KeyCode::Down) => self.preset = (self.preset + 1).min(PRESETS.len() - 1),
            (2, KeyCode::Enter) => {
                if let Some(preset) = presets::get(PRESETS[self.preset].name) {
                    // The font answer stands over the preset's glyphs.
                    doc.edit(|c, _| {
                        let glyphs = c.glyphs;
                        *c = presets::apply(c, &preset, Parts::All);
                        c.glyphs = glyphs;
                    });
                }
                self.step = 3;
            }
            (3, KeyCode::Char('y' | 'Y')) => {
                if let Ok(Some((base, draft))) = self.connection()
                    && let Err(e) = draft.save(&base)
                {
                    return Reply::Then(Action::Error(format!("无法写入 settings.json：{e}")));
                }
                return self.finish();
            }
            (3, KeyCode::Char('s' | 'S') | KeyCode::Enter) => return self.finish(),
            _ => {}
        }
        Reply::Stay
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
    fn walks_through_and_connects() {
        let (dir, paths) = temp_paths("setup");
        let settings = dir.join("settings.json");
        let mut doc = Doc::open(paths);
        let mut s = Setup::new(settings.clone());
        s.handle(key(KeyCode::Enter), &mut doc);
        s.handle(key(KeyCode::Char('u')), &mut doc);
        assert_eq!(doc.config.glyphs, GlyphTier::Unicode);
        s.handle(key(KeyCode::Down), &mut doc);
        s.handle(key(KeyCode::Enter), &mut doc);
        assert_eq!(doc.config.preset.as_deref(), Some(PRESETS[s.preset].name));
        assert_eq!(doc.config.glyphs, GlyphTier::Unicode);
        assert!(matches!(
            s.handle(key(KeyCode::Char('y')), &mut doc),
            Reply::Then(Action::Save { .. })
        ));
        let line = Settings::load(&settings).unwrap().status_line().unwrap();
        assert!(cc_settings::is_ccline(&line.command));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
