//! 有限等待的 CLI 调用；archive 直接限额落盘，不在内存保留第二份。
//! 参考：docs/Directory/README.md（实现）。

use super::*;

pub(super) fn bounded(
    command: Command,
    timeout: Duration,
    cap: usize,
) -> Result<Vec<u8>, HostError> {
    let mut out = Vec::new();
    transfer(command, timeout, cap, &mut out)?;
    Ok(out)
}

pub(super) fn archive(
    command: Command,
    timeout: Duration,
    cap: usize,
    path: &Path,
) -> Result<(), HostError> {
    let mut file = File::create(path).map_err(host)?;
    transfer(command, timeout, cap, &mut file)?;
    file.sync_all().map_err(host)
}

fn nonblocking(fd: &impl AsRawFd) -> Result<(), HostError> {
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(host(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn chunk(
    input: &mut impl Read,
    output: &mut impl Write,
    remaining: &mut usize,
) -> Result<bool, HostError> {
    let mut buffer = [0; 8192];
    match input.read(&mut buffer) {
        Ok(0) => Ok(true),
        Ok(n) => {
            *remaining = remaining
                .checked_sub(n)
                .ok_or_else(|| host("CLI output budget exhausted"))?;
            output.write_all(&buffer[..n]).map_err(host)?;
            Ok(false)
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(host(e)),
    }
}

fn transfer(
    mut command: Command,
    timeout: Duration,
    cap: usize,
    output: &mut impl Write,
) -> Result<(), HostError> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = command.spawn().map_err(host)?;
    let result = (|| {
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let deadline = Instant::now() + timeout;
        let mut stdout_left = cap;
        let mut stderr_left = 65536;
        let mut error = Vec::new();
        let mut stdout_done = false;
        let mut stderr_done = false;
        loop {
            // One chunk per stream per iteration: continuous output cannot postpone the deadline.
            if !stdout_done {
                stdout_done = chunk(&mut stdout, output, &mut stdout_left)?;
            }
            if !stderr_done {
                stderr_done = chunk(&mut stderr, &mut error, &mut stderr_left)?;
            }
            if let Some(status) = child.try_wait().map_err(host)? {
                if stdout_done && stderr_done {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(host(format!(
                            "CLI failed: {}",
                            String::from_utf8_lossy(&error)
                        )))
                    };
                }
            }
            if Instant::now() >= deadline {
                return Err(host("CLI timeout; attempt retained"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

/// Task stdout/stderr are drained while running; only the configured prefix is retained.
pub(super) fn drain(
    mut input: impl Read + Send + 'static,
    cap: usize,
) -> std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut total = 0usize;
        let mut buffer = [0; 8192];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total = total.saturating_add(n);
            out.extend_from_slice(&buffer[..n.min(cap.saturating_sub(out.len()))]);
        }
        Ok((out, total > cap))
    })
}
