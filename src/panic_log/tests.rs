use super::*;
use std::process::Command;

const CHILD_MODE: &str = "EDOLTERM_PANIC_TEST_MODE";
const MESSAGE: &str = "controlled panic 한글 🙂";

#[test]
fn panic_child() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    install();
    match mode.as_str() {
        "normal" => {}
        "worker" => {
            let worker = thread::Builder::new()
                .name("test-worker".into())
                .spawn(|| panic!("{MESSAGE}"))
                .unwrap();
            assert!(worker.join().is_err());
        }
        "panic" | "fallback" => panic!("{MESSAGE}"),
        _ => panic!("unknown child mode: {mode}"),
    }
}

#[test]
fn writes_reports_only_for_panics_and_falls_back_when_needed() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "edolterm-panic-test-{}-{nonce}",
        std::process::id()
    ));
    for mode in ["normal", "panic", "worker", "fallback"] {
        let directory = root.join(mode);
        fs::create_dir_all(&directory).unwrap();
        let primary = directory.join("logs");
        if mode == "fallback" {
            fs::write(&primary, "blocks creation of the log directory").unwrap();
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panic_log::tests::panic_child", "--nocapture"])
            .env(CHILD_MODE, mode)
            .env("EDOLTERM_LOG_DIR", &primary)
            .env("TMP", &directory)
            .env("TEMP", &directory)
            .env("TMPDIR", &directory)
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            matches!(mode, "normal" | "worker"),
            "{mode}: {stderr}"
        );
        if mode == "normal" {
            assert!(!primary.exists());
            continue;
        }

        assert!(stderr.contains(MESSAGE), "{mode}: {stderr}");
        let reports = if mode == "fallback" {
            directory.join("edolterm/logs")
        } else {
            primary
        };
        let mut files = fs::read_dir(&reports).unwrap();
        let path = files.next().unwrap().unwrap().path();
        assert!(files.next().is_none());
        assert!(path.to_string_lossy().ends_with(".panic.log"));
        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains(MESSAGE));
        assert!(contents.contains("tests.rs:"));
        assert!(contents.contains("Backtrace:\n"));
        if mode == "worker" {
            assert!(contents.contains("Thread: test-worker"));
        }
    }
    // Remove only the files created by this test.
    fs::remove_dir_all(root).unwrap();
}
