use std::time::{Duration, Instant};

use ratatui::{
    text::{Line, Span},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};

pub struct Spinner<'a> {
    span: Span<'a>,
    block: Option<Block<'a>>,
}

impl<'a> Spinner<'a> {
    pub fn new(span: Span<'a>) -> Self {
        Self { span, block: None }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for Spinner<'_> {
    type State = SpinnerState;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let frame = MINIDOT_FRAMES
            .get((state.frame) % MINIDOT_FRAMES.len())
            .expect("to find a valid static frame");

        let line = Line::from(vec![Span::from(*frame), Span::from(" "), self.span]);

        let para = Paragraph::new(vec![line]);
        let para = if let Some(block) = self.block {
            para.block(block)
        } else {
            para
        };

        para.render(area, buf)
    }
}

pub struct SpinnerState {
    last_event: Instant,
    interval: Duration,
    frame: usize,
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self {
            last_event: Instant::now(),
            interval: Duration::from_millis(1000 / 12),
            frame: 0,
        }
    }
}

const MINIDOT_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl SpinnerState {
    pub fn update(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_event) >= self.interval {
            self.last_event = now;
            self.next_state();
        }
    }

    fn next_state(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }
}
