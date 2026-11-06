use std::process::Output;
use tokio::process::Command;

#[cfg(unix)]
static FOREGROUND_PARENT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(not(unix))]
pub async fn output(command: &mut Command) -> anyhow::Result<Output> {
    Ok(command.kill_on_drop(true).output().await?)
}

#[cfg(unix)]
pub async fn output(command: &mut Command) -> anyhow::Result<Output> {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::{Pid, getpgrp, tcgetpgrp},
    };
    use tokio::signal::unix::{SignalKind, signal};

    if !crate::terminal::has_display() {
        return Ok(command.kill_on_drop(true).output().await?);
    }
    let terminal = std::fs::File::open("/dev/tty")
        .ok()
        .filter(|tty| tcgetpgrp(tty).ok() == Some(getpgrp()));
    // Register before spawn so a prompt's SIGTTIN cannot race the listener.
    let mut child_signals = signal(SignalKind::child())?;
    command
        .process_group(0)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn()?;
    let pid = Pid::from_raw(child.id().expect("new child has a pid") as i32);
    let mut group = ProcessGroup(Some(pid));
    let mut foreground = None;
    let output = child.wait_with_output();
    tokio::pin!(output);
    loop {
        tokio::select! {
            result = &mut output => {
                let result = result?;
                group.0 = None;
                return Ok(result);
            }
            _ = child_signals.recv(), if terminal.is_some() => {
                if let Some(stop) = stop_signal(pid)? {
                    if !matches!(stop, nix::libc::SIGTTIN | nix::libc::SIGTTOU) || foreground.is_some() {
                        drop(foreground.take());
                        crate::terminal::leave_display().await?;
                        nix::sys::signal::kill(nix::unistd::getpid(), Signal::SIGSTOP)?;
                        eprintln!("resuming clone command; authentication input continues");
                    }
                    crate::terminal::suspend_display().await?;
                    foreground = Some(Foreground::new(pid, terminal.as_ref().expect("display owns tty"))?);
                    killpg(pid, Signal::SIGCONT)?;
                }
            }
        }
    }
}

#[cfg(unix)]
fn stop_signal(pid: nix::unistd::Pid) -> anyhow::Result<Option<i32>> {
    // WNOWAIT observes only stops; Tokio alone owns exit-status collection/reaping.
    let mut info: nix::libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        nix::libc::waitid(
            nix::libc::P_PID,
            pid.as_raw() as nix::libc::id_t,
            &mut info,
            nix::libc::WSTOPPED | nix::libc::WNOWAIT | nix::libc::WNOHANG,
        )
    };
    if result == -1 {
        let error = nix::errno::Errno::last();
        if error == nix::errno::Errno::ECHILD || error == nix::errno::Errno::EINTR {
            return Ok(None);
        }
        return Err(error.into());
    }
    Ok((info.si_code == nix::libc::CLD_STOPPED).then(|| unsafe { info.si_status() }))
}

#[cfg(unix)]
struct ProcessGroup(Option<nix::unistd::Pid>);

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGKILL);
        }
    }
}

#[cfg(unix)]
struct Foreground<'a> {
    previous: nix::unistd::Pid,
    terminal: &'a std::fs::File,
}

#[cfg(unix)]
impl<'a> Foreground<'a> {
    fn new(pid: nix::unistd::Pid, terminal: &'a std::fs::File) -> anyhow::Result<Self> {
        let previous = nix::unistd::tcgetpgrp(terminal)?;
        FOREGROUND_PARENT.store(previous.as_raw(), std::sync::atomic::Ordering::SeqCst);
        if let Err(error) = set_foreground(terminal, pid) {
            FOREGROUND_PARENT.store(0, std::sync::atomic::Ordering::SeqCst);
            return Err(error.into());
        }
        Ok(Self { previous, terminal })
    }
}

#[cfg(unix)]
impl Drop for Foreground<'_> {
    fn drop(&mut self) {
        if FOREGROUND_PARENT.swap(0, std::sync::atomic::Ordering::SeqCst) != 0 {
            let _ = set_foreground(self.terminal, self.previous);
        }
    }
}

#[cfg(unix)]
pub fn restore_foreground() {
    let previous = FOREGROUND_PARENT.swap(0, std::sync::atomic::Ordering::SeqCst);
    if previous != 0
        && let Ok(terminal) = std::fs::File::open("/dev/tty")
    {
        let _ = set_foreground(&terminal, nix::unistd::Pid::from_raw(previous));
    }
}

#[cfg(unix)]
fn set_foreground(terminal: &std::fs::File, pid: nix::unistd::Pid) -> nix::Result<()> {
    use nix::sys::signal::{SigSet, SigmaskHow, Signal, pthread_sigmask};
    let mut blocked = SigSet::empty();
    blocked.add(Signal::SIGTTOU);
    let mut previous = SigSet::empty();
    pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&blocked), Some(&mut previous))?;
    let result = nix::unistd::tcsetpgrp(terminal, pid);
    pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&previous), None)?;
    result
}
