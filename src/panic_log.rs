//! Write a local report when a Rust panic reaches the process-wide hook.
use std::{
    backtrace::Backtrace,
    fs::{self, File},
    io::{self, Write},
    panic,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn install() {
    let previous = panic::take_hook();
    let reports = AtomicUsize::new(0);
    panic::set_hook(Box::new(move |info| {
        save_report(info, reports.fetch_add(1, Ordering::Relaxed));
        previous(info);
    }));
}

fn save_report(info: &panic::PanicHookInfo<'_>, sequence: usize) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    let thread = thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    let version = env!("CARGO_PKG_VERSION");
    let backtrace = Backtrace::force_capture();
    let report = format!(
        "Edolterm {version} | Unix time (ms): {timestamp} | PID: {pid}\n\
         Thread: {thread_name} ({:?})\n{info}\n\nBacktrace:\n{backtrace}\n",
        thread.id()
    );
    let filename = format!("edolterm-{timestamp}-{pid}-{sequence}.panic.log");

    let directory = directory();
    if let Err(error) = write_report(&directory, &filename, &report)
        && let Err(fallback_error) = write_report(
            &std::env::temp_dir().join("edolterm/logs"),
            &filename,
            &report,
        )
    {
        let _ = writeln!(
            io::stderr().lock(),
            "Unable to save panic report in {} ({error}) or in the temporary directory ({fallback_error})",
            directory.display()
        );
    }
}

fn directory() -> PathBuf {
    if let Some(path) = std::env::var_os("EDOLTERM_LOG_DIR") {
        return path.into();
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("Edolterm/Logs");
    }
    crate::config::directory().join("logs")
}

fn write_report(directory: &Path, filename: &str, report: &str) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(directory.join(filename))?;
    file.write_all(report.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests;
