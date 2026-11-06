use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Padding},
};

use super::spinner::{Spinner, SpinnerState};
use crate::terminal::TerminalSession;

pub struct ProgressCommand {
    spinner: SpinnerState,
    heading: String,
}

impl ProgressCommand {
    pub fn new(heading: impl Into<String>) -> Self {
        Self {
            spinner: SpinnerState::default(),
            heading: heading.into(),
        }
    }

    pub async fn execute<F, Fut>(&mut self, func: F) -> anyhow::Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<()>>,
    {
        #[cfg(unix)]
        let interrupts = crate::terminal::InterruptGuard::new()?;
        let mut session = TerminalSession::new()?;
        session.terminal.draw(|frame| self.draw(frame))?;
        let mut suspended = false;
        let (display, mut handoffs) = tokio::sync::mpsc::unbounded_channel();
        let work = crate::terminal::DISPLAY.scope(display, func());
        tokio::pin!(work);
        let mut frames = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! {
                result = &mut work => return result,
                Some(request) = handoffs.recv() => {
                    let ready = match request {
                        crate::terminal::DisplayRequest::Suspend(ready) => {
                            session.suspend()?;
                            ready
                        }
                        crate::terminal::DisplayRequest::Leave(ready) => {
                            session.leave();
                            ready
                        }
                    };
                    suspended = true;
                    let _ = ready.send(());
                }
                _ = frames.tick() => {
                    #[cfg(unix)]
                    if interrupts.interrupted() {
                        anyhow::bail!("clone cancelled; any partial checkout has been kept");
                    }
                    if suspended {
                        continue;
                    }
                    if event::poll(Duration::ZERO)?
                        && let Event::Key(key) = event::read()?
                        && (key.code == KeyCode::Esc
                            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
                    {
                        anyhow::bail!("clone cancelled; any partial checkout has been kept");
                    }
                    self.spinner.update();
                    session.terminal.draw(|frame| self.draw(frame))?;
                }
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        StatefulWidget::render(
            Spinner::new(Span::from(&self.heading))
                .block(Block::new().padding(Padding::symmetric(2, 1))),
            frame.area(),
            frame.buffer_mut(),
            &mut self.spinner,
        );
    }
}
