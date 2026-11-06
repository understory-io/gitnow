use std::io::{self, IsTerminal, stdout};
use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};

use crossterm::{
    cursor::Show,
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK: Once = Once::new();

pub enum DisplayRequest {
    Suspend(tokio::sync::oneshot::Sender<()>),
    Leave(tokio::sync::oneshot::Sender<()>),
}

tokio::task_local! {
    pub static DISPLAY: tokio::sync::mpsc::UnboundedSender<DisplayRequest>;
}

pub fn has_display() -> bool {
    DISPLAY.try_with(|_| ()).is_ok()
}

pub async fn suspend_display() -> anyhow::Result<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    if let Ok(result) = DISPLAY.try_with(|display| display.send(DisplayRequest::Suspend(send))) {
        result.map_err(|_| anyhow::anyhow!("progress display closed before terminal handoff"))?;
        receive.await?;
    }
    Ok(())
}

pub async fn leave_display() -> anyhow::Result<()> {
    let (send, receive) = tokio::sync::oneshot::channel();
    if let Ok(result) = DISPLAY.try_with(|display| display.send(DisplayRequest::Leave(send))) {
        result.map_err(|_| anyhow::anyhow!("progress display closed before suspension"))?;
        receive.await?;
    }
    Ok(())
}

pub struct TerminalSession {
    pub terminal: ratatui::DefaultTerminal,
    _guard: TerminalGuard,
}

impl TerminalSession {
    pub fn leave(&mut self) {
        restore();
    }

    pub fn suspend(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(stdout(), Show)?;
        Ok(())
    }

    pub fn new() -> anyhow::Result<Self> {
        anyhow::ensure!(
            io::stdin().is_terminal() && io::stdout().is_terminal(),
            "interactive display requires a terminal on stdin and stdout; use a query with --no-shell"
        );
        PANIC_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                previous(info);
            }));
        });
        anyhow::ensure!(
            !ACTIVE.swap(true, Ordering::SeqCst),
            "terminal already in use"
        );
        let guard = TerminalGuard;
        // Fullscreen has a known origin. Inline viewports require a DSR reply from stdin.
        crossterm::terminal::enable_raw_mode()?;
        execute!(stdout(), crossterm::terminal::EnterAlternateScreen)?;
        let terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;
        Ok(Self {
            terminal,
            _guard: guard,
        })
    }
}

fn restore() {
    #[cfg(unix)]
    crate::subprocess::restore_foreground();
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), Show, LeaveAlternateScreen);
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

#[cfg(unix)]
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn interrupt(_: nix::libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
pub struct InterruptGuard(nix::sys::signal::SigAction);

#[cfg(unix)]
impl InterruptGuard {
    pub fn new() -> anyhow::Result<Self> {
        use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
        INTERRUPTED.store(false, Ordering::SeqCst);
        let action = SigAction::new(
            SigHandler::Handler(interrupt),
            SaFlags::SA_RESTART,
            SigSet::empty(),
        );
        Ok(Self(unsafe { sigaction(Signal::SIGINT, &action)? }))
    }

    pub fn interrupted(&self) -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }
}

#[cfg(unix)]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        let _ = unsafe { nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGINT, &self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an owned PTY; run scripts/terminal-hooks-proof.py --terminal-test-binary"]
    fn unwinding_restores_the_terminal_before_panic_output() {
        let result = std::panic::catch_unwind(|| {
            let mut session = TerminalSession::new().unwrap();
            session
                .terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new("panic-probe"),
                        frame.area(),
                    );
                })
                .unwrap();
            panic!("owned terminal panic probe");
        });
        assert!(result.is_err());
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
        assert!(!ACTIVE.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires an owned PTY; run scripts/terminal-hooks-proof.py --terminal-test-binary"]
    fn ctrl_c_during_cooked_foreground_handoff_keeps_cleanup_running() {
        use nix::sys::signal::{Signal, raise};
        let interrupts = InterruptGuard::new().unwrap();
        let mut session = TerminalSession::new().unwrap();
        session.suspend().unwrap();
        println!("cooked-foreground-probe");
        raise(Signal::SIGINT).unwrap();
        assert!(interrupts.interrupted());
        drop(session);
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    }
}
