use crate::terminal::TerminalSession;
use app::App;

use crate::git_provider::Repository;

pub trait Searchable: Clone {
    fn display_label(&self) -> String;
}

impl Searchable for Repository {
    fn display_label(&self) -> String {
        self.to_rel_path().display().to_string()
    }
}

#[derive(Clone)]
pub struct StringItem(pub String);

impl Searchable for StringItem {
    fn display_label(&self) -> String {
        self.0.clone()
    }
}

pub struct Interactive {
    app: &'static crate::app::App,
}

impl Interactive {
    pub fn new(app: &'static crate::app::App) -> Self {
        Self { app }
    }

    pub fn interactive_search(
        &mut self,
        repositories: &[Repository],
        initial_query: &str,
    ) -> anyhow::Result<Option<Repository>> {
        self.interactive_search_items(repositories, initial_query)
    }

    pub fn interactive_search_items<T: Searchable>(
        &mut self,
        items: &[T],
        initial_query: &str,
    ) -> anyhow::Result<Option<T>> {
        let mut session = TerminalSession::new()?;
        App::new(self.app, items, initial_query).run(&mut session.terminal)
    }

    pub fn interactive_multi_search<T: Searchable>(
        &mut self,
        items: &[T],
    ) -> anyhow::Result<Vec<T>> {
        let mut session = TerminalSession::new()?;
        multi_select::MultiSelectApp::new(self.app, items).run(&mut session.terminal)
    }
}

pub trait InteractiveApp {
    fn interactive(&self) -> Interactive;
}

impl InteractiveApp for &'static crate::app::App {
    fn interactive(&self) -> Interactive {
        Interactive::new(self)
    }
}

mod app {
    use crossterm::event::KeyModifiers;
    use ratatui::{
        DefaultTerminal, Frame,
        crossterm::event::{self, Event, KeyCode},
        layout::{Constraint, Layout},
        style::{Style, Stylize},
        text::{Line, Span},
        widgets::{ListItem, ListState, Paragraph, StatefulWidget},
    };

    use crate::fuzzy_matcher::FuzzyMatcherApp;

    use super::Searchable;

    pub struct App<'a, T: Searchable> {
        app: &'static crate::app::App,
        items: &'a [T],
        current_search: String,
        matched_items: Vec<T>,
        list: ListState,
    }

    impl<'a, T: Searchable> App<'a, T> {
        pub fn new(app: &'static crate::app::App, items: &'a [T], initial_query: &str) -> Self {
            Self {
                app,
                items,
                current_search: initial_query.to_owned(),
                matched_items: Vec::default(),
                list: ListState::default(),
            }
        }

        fn update_matched_items(&mut self) {
            let labels: Vec<String> = self.items.iter().map(|item| item.display_label()).collect();
            let matched_indices = self
                .app
                .fuzzy_matcher()
                .match_indices(&self.current_search, &labels);

            self.matched_items = matched_indices
                .into_iter()
                .map(|index| self.items[index].clone())
                .collect();

            if self.matched_items.is_empty() {
                self.list.select(None);
            } else {
                self.list.select_first();
            }
        }

        pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<Option<T>> {
            self.update_matched_items();

            loop {
                terminal.draw(|frame| self.draw(frame))?;

                if let Event::Key(key) = event::read()? {
                    if let KeyCode::Char('c') = key.code
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(None);
                    }

                    match key.code {
                        KeyCode::Char(letter) => {
                            self.current_search.push(letter);
                            self.update_matched_items();
                        }
                        KeyCode::Backspace => {
                            if self.current_search.pop().is_some() {
                                self.update_matched_items();
                            }
                        }
                        KeyCode::Esc => {
                            return Ok(None);
                        }
                        KeyCode::Enter => {
                            if let Some(selected) = self.list.selected()
                                && let Some(item) = self.matched_items.get(selected).cloned()
                            {
                                return Ok(Some(item));
                            }

                            return Ok(None);
                        }
                        KeyCode::Up => self.list.select_next(),
                        KeyCode::Down => self.list.select_previous(),
                        _ => {}
                    }
                }
            }
        }

        fn draw(&mut self, frame: &mut Frame) {
            let vertical = Layout::vertical([Constraint::Percentage(100), Constraint::Min(1)]);
            let [list_area, input_area] = vertical.areas(frame.area());

            let display_items: Vec<String> = self
                .matched_items
                .iter()
                .map(|i| i.display_label())
                .collect();

            let list_items: Vec<ListItem> = display_items.into_iter().map(ListItem::from).collect();

            let list = ratatui::widgets::List::new(list_items)
                .direction(ratatui::widgets::ListDirection::BottomToTop)
                .scroll_padding(3)
                .highlight_symbol("> ")
                .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
                .highlight_style(Style::default().bold().white());

            StatefulWidget::render(list, list_area, frame.buffer_mut(), &mut self.list);

            let input = Paragraph::new(Line::from(vec![
                Span::from("> ").blue(),
                Span::from(self.current_search.as_str()),
                Span::from(" ").on_white(),
            ]));

            frame.render_widget(input, input_area);
        }
    }
}

pub mod multi_select {
    use std::collections::HashSet;

    use crossterm::event::KeyModifiers;
    use ratatui::{
        DefaultTerminal, Frame,
        crossterm::event::{self, Event, KeyCode},
        layout::{Constraint, Layout},
        style::{Style, Stylize},
        text::{Line, Span},
        widgets::{ListItem, ListState, Paragraph, StatefulWidget},
    };

    use crate::fuzzy_matcher::FuzzyMatcherApp;

    use super::Searchable;

    pub struct MultiSelectApp<'a, T: Searchable> {
        app: &'static crate::app::App,
        items: &'a [T],
        current_search: String,
        matched_items: Vec<T>,
        selected_labels: HashSet<String>,
        list: ListState,
    }

    impl<'a, T: Searchable> MultiSelectApp<'a, T> {
        pub fn new(app: &'static crate::app::App, items: &'a [T]) -> Self {
            Self {
                app,
                items,
                current_search: String::default(),
                matched_items: Vec::default(),
                selected_labels: HashSet::new(),
                list: ListState::default(),
            }
        }

        fn update_matched_items(&mut self) {
            let labels: Vec<String> = self.items.iter().map(|item| item.display_label()).collect();
            let matched_indices = self
                .app
                .fuzzy_matcher()
                .match_indices(&self.current_search, &labels);

            self.matched_items = matched_indices
                .into_iter()
                .map(|index| self.items[index].clone())
                .collect();

            if self.matched_items.is_empty() {
                self.list.select(None);
            } else {
                self.list.select_first();
            }
        }

        fn toggle_current(&mut self) {
            if let Some(selected) = self.list.selected()
                && let Some(item) = self.matched_items.get(selected)
            {
                let label = item.display_label();
                if !self.selected_labels.remove(&label) {
                    self.selected_labels.insert(label);
                }
            }
        }

        pub fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<Vec<T>> {
            self.update_matched_items();

            loop {
                terminal.draw(|frame| self.draw(frame))?;

                if let Event::Key(key) = event::read()? {
                    if let KeyCode::Char('c') = key.code
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        return Ok(Vec::new());
                    }

                    match key.code {
                        KeyCode::Tab => {
                            self.toggle_current();
                        }
                        KeyCode::Char(letter) => {
                            self.current_search.push(letter);
                            self.update_matched_items();
                        }
                        KeyCode::Backspace => {
                            if self.current_search.pop().is_some() {
                                self.update_matched_items();
                            }
                        }
                        KeyCode::Esc => {
                            return Ok(Vec::new());
                        }
                        KeyCode::Enter => {
                            if self.selected_labels.is_empty() {
                                // Don't allow confirming with no selections
                                continue;
                            }
                            let selected: Vec<T> = self
                                .items
                                .iter()
                                .filter(|i| self.selected_labels.contains(&i.display_label()))
                                .cloned()
                                .collect();
                            return Ok(selected);
                        }
                        KeyCode::Up => self.list.select_next(),
                        KeyCode::Down => self.list.select_previous(),
                        _ => {}
                    }
                }
            }
        }

        fn draw(&mut self, frame: &mut Frame) {
            let vertical = Layout::vertical([
                Constraint::Percentage(100),
                Constraint::Min(1),
                Constraint::Min(1),
            ]);
            let [list_area, input_area, hint_area] = vertical.areas(frame.area());

            let list_items: Vec<ListItem> = self
                .matched_items
                .iter()
                .map(|i| {
                    let label = i.display_label();
                    let marker = if self.selected_labels.contains(&label) {
                        "[x] "
                    } else {
                        "[ ] "
                    };
                    ListItem::from(Line::from(vec![
                        Span::from(marker).green(),
                        Span::from(label),
                    ]))
                })
                .collect();

            let list = ratatui::widgets::List::new(list_items)
                .direction(ratatui::widgets::ListDirection::BottomToTop)
                .scroll_padding(3)
                .highlight_symbol("> ")
                .highlight_spacing(ratatui::widgets::HighlightSpacing::Always)
                .highlight_style(Style::default().bold().white());

            StatefulWidget::render(list, list_area, frame.buffer_mut(), &mut self.list);

            let input = Paragraph::new(Line::from(vec![
                Span::from("> ").blue(),
                Span::from(self.current_search.as_str()),
                Span::from(" ").on_white(),
            ]));
            frame.render_widget(input, input_area);

            let count = self.selected_labels.len();
            let hint = Paragraph::new(Line::from(vec![
                Span::from(format!("{count} selected")).dim(),
                Span::from(" | Tab: toggle, Enter: confirm").dim(),
            ]));
            frame.render_widget(hint, hint_area);
        }
    }
}
