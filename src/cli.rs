use anyhow::{Context, Result, bail};
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub struct Options {
    pub working_directory: Option<PathBuf>,
    pub new_window: bool,
    #[cfg(debug_assertions)]
    pub smoke_mode: Option<String>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--new-window" {
                options.new_window = true;
                continue;
            }
            let directory = if arg == "--working-directory" || arg == "-d" {
                args.next()
                    .context("--working-directory 뒤에 폴더 경로를 지정하세요.")?
            } else if let Some(path) = arg
                .to_str()
                .and_then(|s| s.strip_prefix("--working-directory="))
            {
                OsString::from(path)
            } else {
                #[cfg(debug_assertions)]
                if let Some(mode) = arg.to_str().and_then(|s| s.strip_prefix("--smoke-test=")) {
                    options.smoke_mode = Some(mode.into());
                    continue;
                }
                bail!("알 수 없는 실행 인자: {}", arg.to_string_lossy());
            };
            if options.working_directory.is_some() {
                bail!("시작 폴더는 한 번만 지정할 수 있습니다.");
            }
            if directory.is_empty() {
                bail!("시작 폴더 경로가 비어 있습니다.");
            }
            let path = std::path::absolute(PathBuf::from(directory))?;
            if !path.is_dir() {
                bail!("시작 폴더를 찾을 수 없습니다: {}", path.display());
            }
            options.working_directory = Some(path);
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options> {
        Options::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn explicit_new_window_can_be_combined_with_directory() {
        let options = parse(&["--new-window", "-d", "."]).unwrap();
        assert!(options.new_window);
        assert!(options.working_directory.is_some());
        assert!(!parse(&[]).unwrap().new_window);
    }

    #[test]
    fn launch_directory_preserves_unicode_spaces_and_shell_characters() {
        let root = std::env::temp_dir().join(format!("edolterm-cli-{}", std::process::id()));
        let directory = root.join("한글 폴더 it's $value & [1]; %PATH%");
        std::fs::create_dir_all(&directory).unwrap();
        let args = [
            OsString::from("--working-directory"),
            directory.join(".").into_os_string(),
        ];
        let actual = Options::parse(args).unwrap().working_directory.unwrap();
        assert_eq!(
            actual.canonicalize().unwrap(),
            directory.canonicalize().unwrap()
        );
        let equals = format!("--working-directory={}", directory.display());
        assert_eq!(
            parse(&[&equals]).unwrap().working_directory.unwrap(),
            directory
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn launch_directory_rejects_missing_invalid_or_duplicate_arguments() {
        assert!(parse(&["-d"]).is_err());
        assert!(parse(&["--working-directory="]).is_err());
        assert!(parse(&["-d", "__edolterm_missing_directory__"]).is_err());
        assert!(parse(&["-d", ".", "-d", "."]).is_err());
        assert!(parse(&["--unknown-option"]).is_err());
        assert!(parse(&[]).unwrap().working_directory.is_none());
    }

    #[test]
    fn launch_directory_accepts_relative_paths_and_drive_roots() {
        assert_eq!(
            parse(&["-d", "."]).unwrap().working_directory.unwrap(),
            std::env::current_dir().unwrap()
        );
        #[cfg(windows)]
        {
            let directory = std::env::current_dir().unwrap();
            let root = directory.ancestors().last().unwrap();
            assert_eq!(
                parse(&["-d", root.to_str().unwrap()])
                    .unwrap()
                    .working_directory
                    .unwrap(),
                root
            );
        }
    }
}
