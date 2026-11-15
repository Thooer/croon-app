//! Linux 文件操作：所有后续名字都相对于已打开且不跟随链接的目录 FD。
use std::ffi::CString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

fn c(bytes: &[u8]) -> io::Result<CString> {
    CString::new(bytes).map_err(|_| io::Error::other("NUL in path"))
}
fn opened(fd: libc::c_int) -> io::Result<File> {
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
pub(crate) fn root(path: &Path) -> io::Result<File> {
    let path = c(path.as_os_str().as_bytes())?;
    opened(unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_RDONLY | libc::O_CLOEXEC,
        )
    })
}
pub(crate) fn open(dir: &File, name: &str, directory: bool) -> io::Result<File> {
    let name = c(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    opened(unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) })
}
pub(crate) fn names(dir: &File) -> io::Result<Vec<String>> {
    // /proc/self/fd names the already-open directory, including after a rename.
    fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))?
        .map(|entry| {
            entry?
                .file_name()
                .into_string()
                .map_err(|_| io::Error::other("non-Unicode name"))
        })
        .collect()
}
pub(crate) fn metadata(dir: &File, name: &str) -> io::Result<fs::Metadata> {
    fs::symlink_metadata(format!("/proc/self/fd/{}/{name}", dir.as_raw_fd()))
}
pub(crate) fn link(dir: &File, name: &str) -> io::Result<String> {
    fs::read_link(format!("/proc/self/fd/{}/{name}", dir.as_raw_fd()))?
        .into_os_string()
        .into_string()
        .map_err(|_| io::Error::other("non-Unicode link"))
}
pub(crate) fn mkdir(dir: &File, name: &str) -> io::Result<File> {
    let name_c = c(name.as_bytes())?;
    if unsafe { libc::mkdirat(dir.as_raw_fd(), name_c.as_ptr(), 0o700) } != 0 {
        return Err(io::Error::last_os_error());
    }
    open(dir, name, true)
}
pub(crate) fn write(dir: &File, name: &str, bytes: &[u8], executable: bool) -> io::Result<()> {
    let name = c(name.as_bytes())?;
    let mut file = opened(unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            if executable { 0o755 } else { 0o644 },
        )
    })?;
    file.write_all(bytes)
}
pub(crate) fn symlink(dir: &File, name: &str, target: &str) -> io::Result<()> {
    let name = c(name.as_bytes())?;
    let target = c(target.as_bytes())?;
    if unsafe { libc::symlinkat(target.as_ptr(), dir.as_raw_fd(), name.as_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub(crate) fn bytes(file: File, limit: usize) -> io::Result<Vec<u8>> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other("directory byte budget exhausted"));
    }
    Ok(bytes)
}
