//! 純文字終端機繪製與輸入（GitStatus 風格：`=====` 分隔線、`>` 游標、底部按鍵說明）。
//!
//! 畫面從左上角逐行輸出，不使用視窗元件。

use std::io::{Stdout, Write, stdout};

use anyhow::Result;
pub use crossterm::event::KeyCode;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, style::Print, terminal};

const RULE: &str = "==============================================================================";

pub struct Term {
    out: Stdout,
    app: &'static str,
}

impl Term {
    /// `app` 顯示在每個畫面的標題前，例如 `WHAT Plus Server`。
    pub fn enter(app: &'static str) -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self { out, app })
    }

    fn width() -> usize {
        terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(120)
            .max(20)
    }

    /// 清除畫面並輸出標題、內容與按鍵說明。
    pub fn draw(&mut self, title: &str, lines: &[String], footer: &str) -> Result<()> {
        let width = Self::width();
        let fit = |s: &str| -> String { s.chars().take(width.saturating_sub(1)).collect() };
        queue!(
            self.out,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        let mut all = vec![
            RULE.to_string(),
            format!(" {} - {title}", self.app),
            RULE.to_string(),
            String::new(),
        ];
        all.extend(lines.iter().cloned());
        all.push(String::new());
        all.push(RULE.to_string());
        all.push(format!(" {footer}"));
        for line in all {
            queue!(self.out, Print(fit(&line)), Print("\r\n"))?;
        }
        self.out.flush()?;
        Ok(())
    }

    /// 等待一個按鍵（只回傳按下事件）。Ctrl+C 視為 Esc。
    pub fn key(&mut self) -> Result<KeyEvent> {
        loop {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                }
                return Ok(k);
            }
        }
    }

    /// 顯示訊息，按任意鍵返回。
    pub fn message(&mut self, title: &str, text: &str) -> Result<()> {
        let lines: Vec<String> = text.lines().map(|l| format!(" {l}")).collect();
        self.draw(title, &lines, "Press any key to continue")?;
        self.key()?;
        Ok(())
    }

    pub fn confirm(&mut self, title: &str, text: &str) -> Result<bool> {
        let mut lines: Vec<String> = text.lines().map(|l| format!(" {l}")).collect();
        lines.push(String::new());
        lines.push(" Continue? (y/N)".into());
        self.draw(title, &lines, "y = yes   any other key = no")?;
        Ok(matches!(
            self.key()?.code,
            KeyCode::Char('y') | KeyCode::Char('Y')
        ))
    }

    /// 單行輸入。Enter 確認、Esc 取消。
    pub fn prompt(
        &mut self,
        title: &str,
        help: &[&str],
        label: &str,
        initial: &str,
    ) -> Result<Option<String>> {
        let mut value = initial.to_string();
        loop {
            let mut lines: Vec<String> = help.iter().map(|l| format!(" {l}")).collect();
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!(" {label}: {value}_"));
            self.draw(
                title,
                &lines,
                "Enter = confirm   Esc = cancel   Backspace = delete",
            )?;
            let k = self.key()?;
            match k.code {
                KeyCode::Enter => return Ok(Some(value.trim().to_string())),
                KeyCode::Esc => return Ok(None),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(c) if !c.is_control() => value.push(c),
                _ => {}
            }
        }
    }

    /// 從清單選一項。回傳選中的索引；Esc 回傳 `None`。
    pub fn choose(
        &mut self,
        title: &str,
        header: &[String],
        items: &[String],
        start: usize,
    ) -> Result<Option<usize>> {
        if items.is_empty() {
            return Ok(None);
        }
        let mut cursor = start.min(items.len() - 1);
        loop {
            let mut lines = header.to_vec();
            if !header.is_empty() {
                lines.push(String::new());
            }
            lines.extend(list_lines(items, cursor));
            self.draw(
                title,
                &lines,
                "Up/Down = move   Enter = select   Esc = back",
            )?;
            match self.key()?.code {
                KeyCode::Up => cursor = cursor.saturating_sub(1),
                KeyCode::Down => cursor = (cursor + 1).min(items.len() - 1),
                KeyCode::Enter => return Ok(Some(cursor)),
                KeyCode::Esc => return Ok(None),
                _ => {}
            }
        }
    }
}

/// 帶 `>` 游標的清單行。
pub fn list_lines(items: &[String], cursor: usize) -> Vec<String> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| format!("{} {item}", if i == cursor { ">" } else { " " }))
        .collect()
}

pub fn checkbox(on: bool) -> &'static str {
    if on { "[*]" } else { "[ ]" }
}

impl Drop for Term {
    fn drop(&mut self) {
        let _ = execute!(self.out, cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}
