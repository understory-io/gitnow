#!/usr/bin/env python3
"""Black-box fixture/PTY regressions; usage: script /absolute/path/to/gitnow."""
import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time


def run(args, env, cwd=None):
    return subprocess.run(args, env=env, cwd=cwd, capture_output=True, timeout=20)


def checked(args, env, cwd=None):
    result = run(args, env, cwd)
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    return result


def field(number, value):
    data = value.encode() if isinstance(value, str) else value
    def varint(n):
        result = bytearray()
        while n >= 128:
            result.append((n & 127) | 128)
            n >>= 7
        result.append(n)
        return bytes(result)
    return varint(number * 8 + 2) + varint(len(data)) + data


class Fixture:
    def __init__(self, base, binary):
        self.base = base
        self.binary = binary
        self.env = dict(os.environ, HOME=str(base), XDG_CONFIG_HOME=str(base / 'config'),
                        XDG_CACHE_HOME=str(base / 'cache'), GIT_CONFIG_NOSYSTEM='1',
                        GIT_CONFIG_GLOBAL='/dev/null', JJ_CONFIG=str(base / 'jj.toml'),
                        TERM='xterm-256color', SHELL='/bin/true')
        self.env.pop('GITNOW_CONFIG', None)
        self.env.pop('GITNOW_CHOOSER_FILE', None)
        (base / 'jj.toml').write_text('[user]\nname="Fixture"\nemail="fixture@example.invalid"\n')
        self.origin = base / 'origin'
        checked(['git', 'init', str(self.origin)], self.env)
        checked(['git', '-C', str(self.origin), '-c', 'user.name=Fixture', '-c',
                 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-m', 'fixture'], self.env)
        self.checkout = base / 'projects/local/owner/fixture'
        (base / 'cache').mkdir()
        repo = b''.join(field(n, v) for n, v in enumerate(
            ['local', 'owner', 'fixture', str(self.origin)], 1))
        (base / 'cache/cache.proto').write_bytes(field(1, repo))
        self.config = base / 'gitnow.toml'
        self.configure()

    def configure(self, hooks=''):
        self.config.write_text(f'''[settings]
{hooks}
[settings.projects]
directory = "{self.base / 'projects'}"
[settings.cache]
location = "{self.base / 'cache'}"
duration = false
''')

    def args(self, *args):
        return [self.binary, '--config', str(self.config), *args]

    def clone(self):
        checked(['git', 'clone', str(self.origin), str(self.checkout)], self.env)


def pty_run(fixture, args, reply, key=None, ready=b'fixture', wait_file=None, command=None,
            require_handoff=False, display=True, pipe_stdin=False, resume_key=None):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 100, 0, 0))
    before = termios.tcgetattr(slave)
    def session():
        os.setsid()
        fcntl.ioctl(slave if pipe_stdin else 0, termios.TIOCSCTTY, 0)
    process = subprocess.Popen(command or fixture.args(*args), env=fixture.env,
                               stdin=subprocess.DEVNULL if pipe_stdin else slave,
                               stdout=slave, stderr=slave, preexec_fn=session,
                               pass_fds=(slave,) if pipe_stdin else ())
    output = bytearray()
    sent = False
    pending_reply = None
    queries = 0
    child_fds = []
    saw_stop = False
    sent_resume_key = False
    deadline = time.monotonic() + 12
    try:
        while True:
            now = time.monotonic()
            assert now < deadline, bytes(output)
            if pending_reply is not None and now >= pending_reply:
                os.write(master, b'\x1b[1;1R')
                pending_reply = None
            if select.select([master], [], [], 0.02)[0]:
                try:
                    data = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    data = b''
                output.extend(data)
                count = bytes(output).count(b'\x1b[6n')
                if count > queries:
                    queries = count
                    if reply == 'normal':
                        os.write(master, b'\x1b[1;1R')
                    elif reply == 'delayed':
                        pending_reply = now + 2.2
            handed_off = False
            if require_handoff and process.poll() is None:
                try:
                    handed_off = (termios.tcgetattr(slave) == before
                                  and os.tcgetpgrp(master) != process.pid)
                except OSError as error:
                    if error.errno != errno.ENOTTY:
                        raise
            if (not sent and ready in output and (wait_file is None or wait_file.exists())
                    and (not require_handoff or handed_off)):
                if require_handoff and display:
                    assert output.rfind(b'\x1b[?1049h') > output.rfind(b'\x1b[?1049l'), 'auth prompt was erased'
                if wait_file:
                    child_fds = [os.pidfd_open(int(pid)) for pid in wait_file.read_text().split()]
                sent = True
                if key:
                    os.write(master, key)
            if sent and resume_key is not None and not saw_stop:
                try:
                    stopped = os.waitid(os.P_PID, process.pid, os.WSTOPPED | os.WNOHANG | os.WNOWAIT)
                except ChildProcessError:
                    stopped = None
                if stopped is not None:
                    assert termios.tcgetattr(slave) == before, 'job suspended in raw mode'
                    assert output.rfind(b'\x1b[?1049l') > output.rfind(b'\x1b[?1049h'), 'job suspended on alternate screen'
                    saw_stop = True
                    os.kill(process.pid, signal.SIGCONT)
            if saw_stop and not sent_resume_key and handed_off and b'resuming clone command' in output:
                os.write(master, resume_key)
                sent_resume_key = True
            if process.poll() is not None:
                # Slave remains open for termios assertions, so drain without waiting for EOF.
                while select.select([master], [], [], 0)[0]:
                    output.extend(os.read(master, 65536))
                break
        assert sent, bytes(output)
        if resume_key is not None:
            assert saw_stop and sent_resume_key, ('job control did not resume', bytes(output))
        after = termios.tcgetattr(slave)
        assert before == after, ('terminal attributes not restored', before, after)
        if display:
            assert b'\x1b[?25h' in output, ('cursor not shown', bytes(output))
            assert output.rfind(b'\x1b[?1049l') > output.rfind(b'\x1b[?1049h'), bytes(output)
        assert queries == 0, ('unnecessary cursor query', queries, bytes(output))
        assert b'cursor position could not be read' not in output, bytes(output)
        for child_fd in child_fds:
            assert select.select([child_fd], [], [], 5)[0], 'clone descendant survived cancellation'
        return process.returncode, bytes(output)
    finally:
        if not child_fds and wait_file is not None and wait_file.exists():
            for pid in wait_file.read_text().split():
                try:
                    child_fds.append(os.pidfd_open(int(pid)))
                except ProcessLookupError:
                    pass
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)
        os.close(master)
        os.close(slave)
        for child_fd in child_fds:
            if not select.select([child_fd], [], [], 0)[0]:
                signal.pidfd_send_signal(child_fd, signal.SIGKILL)
            os.close(child_fd)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=lambda p: str(Path(p).resolve()))
    parser.add_argument('--terminal-test-binary', type=lambda p: str(Path(p).resolve()))
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='terminal-hooks-', dir=os.environ['TMPDIR']) as root:
        root = Path(root)
        def fixture(name):
            path = root / name
            path.mkdir()
            return Fixture(path, args.binary)
        f = fixture('hooks')
        f.clone()
        f.configure('post_update_command = ["false"]')
        result = run(f.args('--no-shell', 'fixture'), f.env)
        assert result.returncode != 0, ('failed hook reported success', result.stdout, result.stderr)
        assert (f.checkout / '.git').exists()
        f.configure('''post_update_command = ["jj git fetch"]
post_update_command_by_vcs = { git = ["git fetch origin"] }''')
        checked(f.args('--no-shell', 'fixture'), f.env)
        assert not (f.checkout / '.jj').exists()
        checked(['jj', 'git', 'init', '--colocate', str(f.checkout)], f.env)
        checked(f.args('--no-shell', 'fixture'), f.env)
        f.configure('post_update_command = ["false"]')
        result = run(f.args('--no-shell', 'fixture'), f.env)
        assert result.returncode != 0 and (f.checkout / '.jj').exists()
        f = fixture('clone')
        f.configure('post_clone_command = ["jj git init --colocate"]')
        checked(f.args('--no-shell', 'fixture'), f.env)
        assert (f.checkout / '.jj').exists()
        f = fixture('clone-hook-error')
        f.configure('post_clone_command = ["false"]')
        result = run(f.args('--no-shell', 'fixture'), f.env)
        assert result.returncode != 0 and (f.checkout / '.git').exists()
        print('PASS Git/jj update selection, legacy failure, clone initialization, failed hook preservation')
        for reply in ['normal', 'delayed', 'missing']:
            for key in [b'\r', b'\x1b', b'\x03']:
                f = fixture(f'picker-{reply}-{key.hex()}')
                f.clone()
                code, output = pty_run(f, ['--no-shell', '--interactive', 'fixture'], reply, key)
                assert (code == 0) == (key == b'\r'), output
            f = fixture(f'progress-{reply}')
            f.configure('post_clone_command = ["jj git init --colocate"]')
            code, output = pty_run(f, ['fixture'], reply, ready=b'cloning:')
            assert code == 0 and (f.checkout / '.jj').exists(), output
            print(f'PASS picker select/Esc/Ctrl-C and clone progress: {reply} cursor replies; restored terminal')
            f = fixture(f'transition-{reply}')
            f.configure('post_clone_command = ["jj git init --colocate"]')
            code, output = pty_run(f, ['--interactive', 'fixture'], reply, b'\r')
            assert code == 0 and b'cloning:' in output and (f.checkout / '.jj').exists(), output
            f = fixture(f'multi-picker-{reply}')
            code, output = pty_run(f, ['project', 'create', 'fixture', '--no-template', '--no-shell'],
                                   reply, b'\t\r')
            assert code == 0 and (f.base / '.gitnow/projects/fixture').exists(), output
        for key in [b'\x1b', b'\x03']:
            f = fixture(f'cancel-{key.hex()}')
            fifo = f.base / 'wait'
            os.mkfifo(fifo)
            helper = f.base / 'clone-helper'
            helper.write_text(f'#!/bin/sh\ntrap \"\" HUP\ncat "{fifo}" &\nworker=$!\necho $$ $worker > "{f.base / "child-pid"}"\nwait "$worker"\n')
            helper.chmod(0o755)
            f.configure(f'clone_command = "{helper}"')
            code, output = pty_run(f, ['fixture'], 'missing', key, ready=b'cloning:',
                                   wait_file=f.base / 'child-pid')
            assert code != 0 and not f.checkout.exists(), output
        f = fixture('progress-error')
        f.configure('clone_command = "false"')
        code, output = pty_run(f, ['fixture'], 'missing', ready=b'cloning:')
        assert code != 0 and not f.checkout.exists(), output
        print('PASS progress cancellation/error propagation and terminal restoration')
        f = fixture('progress-hook-error')
        f.configure('post_clone_command = ["false"]')
        code, output = pty_run(f, ['fixture'], 'missing', ready=b'cloning:')
        assert code != 0 and (f.checkout / '.git').exists(), output
        auth_cases = [(False, False, b'fixture-pass\n', None), (True, False, b'fixture-pass\n', None),
                      (True, True, b'fixture-pass\n', None), (False, False, b'\x03', None),
                      (False, False, b'\x1a', b'fixture-pass\n')]
        for no_shell, pipe_stdin, key, resume_key in auth_cases:
            f = fixture(f'auth-{no_shell}-{pipe_stdin}-{key.hex()}')
            helper = f.base / 'auth-helper'
            helper.write_text(f'''#!/bin/sh
echo $$ > "{f.base / 'auth-pid'}"
printf auth-prompt > /dev/tty
read -r answer < /dev/tty
test "$answer" = fixture-pass || exit 7
exec git clone "$1" "$2"
''')
            helper.chmod(0o755)
            shell = f.base / 'shell-helper'
            shell.write_text('''#!/usr/bin/python3
import os, termios
assert os.tcgetpgrp(0) == os.getpgrp()
assert termios.tcgetattr(0)[3] & termios.ICANON
print('shell-restored')
''')
            shell.chmod(0o755)
            f.env['SHELL'] = str(shell)
            f.configure(f'clone_command = "{helper} {{{{ ssh_url }}}} {{{{ path }}}}"')
            command_args = ['--no-shell', 'fixture'] if no_shell else ['fixture']
            code, output = pty_run(f, command_args, 'missing', key, ready=b'auth-prompt',
                                   wait_file=f.base / 'auth-pid', require_handoff=not no_shell,
                                   display=not no_shell, pipe_stdin=pipe_stdin, resume_key=resume_key)
            if key == b'\x03':
                assert code != 0 and not f.checkout.exists(), output
            else:
                assert code == 0 and (f.checkout / '.git').exists(), output
                if not no_shell:
                    assert b'shell-restored' in output, output
        print('PASS /dev/tty authentication handoff with/without progress and foreground restoration')
        if args.terminal_test_binary:
            f = fixture('panic')
            command = [args.terminal_test_binary, '--ignored', '--exact',
                       'terminal::tests::unwinding_restores_the_terminal_before_panic_output',
                       '--nocapture']
            code, output = pty_run(f, [], 'missing', ready=b'panic-probe', command=command)
            assert code == 0, output
            assert output.index(b'\x1b[?1049l') < output.index(b'owned terminal panic probe'), output
            print('PASS panic restores terminal before diagnostics')
            f = fixture('cooked-foreground')
            command = [args.terminal_test_binary, '--ignored', '--exact',
                       'terminal::tests::ctrl_c_during_cooked_foreground_handoff_keeps_cleanup_running',
                       '--nocapture']
            code, output = pty_run(f, [], 'missing', ready=b'cooked-foreground-probe', command=command)
            assert code == 0, output
            print('PASS cooked-foreground SIGINT cleanup')


if __name__ == '__main__':
    main()
