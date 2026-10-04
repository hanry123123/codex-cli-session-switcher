//! Persistent session list shown beside the Codex transcript.

use crate::history_cell::HistoryCell;
use codex_app_server_protocol::Thread;
use codex_protocol::ThreadId;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;

const MIN_WIDTH: u16 = 20;
const MAX_WIDTH: u16 = 34;
const ROW_HEIGHT: usize = 3;
const MAX_CACHED_PREVIEW_CELLS: usize = 100;

pub(crate) const fn is_session_switcher_enabled() -> bool {
    cfg!(feature = "session-switcher") && option_env!("CODEX_SESSION_SWITCHER_BUILD").is_some()
}

#[derive(Debug, Clone)]
pub(crate) struct SessionSwitcherItem {
    pub(crate) thread: Thread,
    pub(crate) archived: bool,
    pub(crate) summary: Option<String>,
    pub(crate) last_user_request: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct SessionSwitcherState {
    pub(crate) items: Vec<SessionSwitcherItem>,
    pub(crate) selected: usize,
    pub(crate) new_session_selected: bool,
    pub(crate) focused: bool,
    pub(crate) activate_on_main_enter: bool,
    pub(crate) loading: bool,
    pub(crate) opening_thread_id: Option<ThreadId>,
    pub(crate) summaries_loading: bool,
    pub(crate) error: Option<String>,
    preview_cache: VecDeque<(ThreadId, Vec<Arc<dyn HistoryCell>>)>,
    recent_threads: Vec<ThreadId>,
    pending_summary_threads: Vec<ThreadId>,
    last_request_loading: HashSet<ThreadId>,
    last_request_loaded: HashSet<ThreadId>,
}

impl SessionSwitcherState {
    pub(crate) fn sidebar_width(screen_width: u16) -> u16 {
        if screen_width < MIN_WIDTH + 28 {
            MIN_WIDTH.min(screen_width / 3)
        } else {
            (screen_width / 4).clamp(MIN_WIDTH, MAX_WIDTH)
        }
    }

    pub(crate) fn contains_column(screen_width: u16, column: u16) -> bool {
        column < Self::sidebar_width(screen_width)
    }

    pub(crate) fn selected_item(&self) -> Option<&SessionSwitcherItem> {
        if self.new_session_selected {
            return None;
        }
        self.items.get(self.selected)
    }

    pub(crate) fn selected_thread_id(&self) -> Option<ThreadId> {
        if self.new_session_selected {
            return None;
        }
        self.selected_item()
            .and_then(|item| ThreadId::try_from(item.thread.id.as_str()).ok())
    }

    pub(crate) fn select_next(&mut self, delta: isize) {
        if delta < 0 {
            if self.new_session_selected {
                return;
            }
            if self.selected == 0 {
                self.new_session_selected = true;
                return;
            }
        } else if self.new_session_selected {
            self.new_session_selected = false;
            self.selected = 0;
            return;
        }
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.items.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub(crate) fn preserve_selection(&mut self, thread_id: Option<ThreadId>) {
        if let Some(thread_id) = thread_id
            && let Some(index) = self
                .items
                .iter()
                .position(|item| item.thread.id == thread_id.to_string())
        {
            self.new_session_selected = false;
            self.selected = index;
        } else if !self.new_session_selected {
            if self.items.is_empty() {
                self.selected = 0;
            } else {
                self.selected = self.selected.min(self.items.len() - 1);
            }
        }
    }

    pub(crate) fn note_thread_switch(&mut self, previous: Option<ThreadId>, current: ThreadId) {
        self.recent_threads
            .retain(|thread_id| *thread_id != current);
        self.recent_threads.insert(0, current);
        if let Some(previous) = previous
            && previous != current
        {
            self.recent_threads
                .retain(|thread_id| *thread_id != previous);
            self.recent_threads
                .insert(1.min(self.recent_threads.len()), previous);
        }
        self.recent_threads.truncate(2);
    }

    pub(crate) fn recent_thread_ids(&self) -> &[ThreadId] {
        &self.recent_threads
    }

    pub(crate) fn cache_thread_preview(
        &mut self,
        thread_id: ThreadId,
        cells: &[Arc<dyn HistoryCell>],
    ) {
        self.preview_cache
            .retain(|(cached_thread_id, _)| *cached_thread_id != thread_id);
        let start = cells.len().saturating_sub(MAX_CACHED_PREVIEW_CELLS);
        self.preview_cache
            .push_front((thread_id, cells[start..].to_vec()));
        self.preview_cache.truncate(2);
    }

    pub(crate) fn cached_thread_preview(
        &self,
        thread_id: ThreadId,
    ) -> Option<&[Arc<dyn HistoryCell>]> {
        self.preview_cache
            .iter()
            .find(|(cached_thread_id, _)| *cached_thread_id == thread_id)
            .map(|(_, cells)| cells.as_slice())
    }

    pub(crate) fn previous_thread(&self, current: Option<ThreadId>) -> Option<ThreadId> {
        self.recent_threads
            .iter()
            .copied()
            .find(|thread_id| Some(*thread_id) != current)
    }

    pub(crate) fn has_recent_threads(&self) -> bool {
        !self.recent_threads.is_empty()
    }

    pub(crate) fn queue_summary_update(&mut self, thread_id: ThreadId) {
        if !self.pending_summary_threads.contains(&thread_id) {
            self.pending_summary_threads.push(thread_id);
        }
    }

    pub(crate) fn take_pending_summary_updates(&mut self) -> Vec<ThreadId> {
        std::mem::take(&mut self.pending_summary_threads)
    }

    pub(crate) fn begin_last_request_load(&mut self, thread_id: ThreadId) -> bool {
        self.last_request_loading.insert(thread_id)
    }

    pub(crate) fn finish_last_request_load(&mut self, thread_id: ThreadId) {
        self.last_request_loading.remove(&thread_id);
    }

    pub(crate) fn mark_last_request_loaded(&mut self, thread_id: ThreadId) {
        self.last_request_loaded.insert(thread_id);
    }

    pub(crate) fn last_request_was_loaded(&self, thread_id: ThreadId) -> bool {
        self.last_request_loaded.contains(&thread_id)
    }

    pub(crate) fn invalidate_last_request(&mut self, thread_id: ThreadId) {
        self.last_request_loaded.remove(&thread_id);
    }

    pub(crate) fn reset_last_request_results(&mut self) {
        self.last_request_loaded.clear();
    }

    pub(crate) fn render(&self, area: Rect, buffer: &mut Buffer, current_thread: Option<ThreadId>) {
        if area.is_empty() {
            return;
        }
        let border = if self.focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let title = if self.focused {
            " Sessions • F6 to return "
        } else {
            " Sessions • F6 focus "
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(title);
        let content_area = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        Clear.render(area, buffer);
        block.render(area, buffer);
        if content_area.is_empty() {
            return;
        }

        let mut lines = Vec::new();
        let width = usize::from(content_area.width.saturating_sub(1));
        let new_session_style = if self.focused && self.new_session_selected {
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        lines.push(Line::styled(
            fit_line("＋ 开始新对话", width),
            new_session_style,
        ));
        if let Some(error) = &self.error {
            lines.push(Line::styled(
                fit_line(&format!("Session list unavailable: {error}"), width),
                Style::default().fg(Color::Red),
            ));
        } else if self.loading && self.items.is_empty() {
            lines.push(Line::styled("Loading sessions…", Style::default().dim()));
        } else if self.items.is_empty() {
            lines.push(Line::styled("No sessions found", Style::default().dim()));
        } else {
            let visible_rows = usize::from(content_area.height).saturating_sub(2) / ROW_HEIGHT;
            let visible_rows = visible_rows.max(1);
            let start = self.selected.saturating_sub(visible_rows.saturating_sub(1));
            for (index, item) in self.items.iter().enumerate().skip(start).take(visible_rows) {
                let thread_id = ThreadId::try_from(item.thread.id.as_str()).ok();
                let is_current = thread_id.is_some() && thread_id == current_thread;
                let is_selected =
                    self.focused && !self.new_session_selected && index == self.selected;
                let marker = if is_current { "● " } else { "  " };
                let name = item
                    .summary
                    .as_deref()
                    .filter(|summary| !summary.trim().is_empty())
                    .or(item.thread.name.as_deref())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Untitled session");
                let heading = fit_line(&format!("{marker}{name}"), width);
                let request = if let Some(request) = item
                    .last_user_request
                    .as_deref()
                    .filter(|request| !request.trim().is_empty())
                {
                    request
                } else if let Some(thread_id) = thread_id
                    && self.last_request_loading.contains(&thread_id)
                {
                    "Loading latest request…"
                } else if thread_id.is_some_and(|id| self.last_request_loaded.contains(&id)) {
                    "No text request"
                } else {
                    item.thread.preview.as_str()
                };
                let request = fit_line(&flatten(request), width);
                let item_style = if is_selected {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else if is_current {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let secondary_style = if is_selected {
                    item_style.add_modifier(Modifier::DIM)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                lines.push(Line::styled(heading, item_style));
                lines.push(Line::styled(request, secondary_style));
                lines.push(Line::styled(
                    if item.archived { "  archived" } else { "" },
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        let footer = if self.focused {
            "↑/↓ move  enter select"
        } else {
            "F6 focus  F7 previous"
        };
        let footer_line = Line::styled(
            fit_line(footer, width),
            Style::default().fg(Color::DarkGray),
        );
        lines.push(footer_line);
        Paragraph::new(lines).render(content_area, buffer);
    }
}

fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fit_line(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let flattened = flatten(text);
    let mut output = String::new();
    for (index, character) in flattened.chars().enumerate() {
        if index >= width {
            output.push('…');
            break;
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
#[path = "session_switcher_tests.rs"]
mod tests;
