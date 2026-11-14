use anyhow::{Context, Result, bail};
use std::{ffi::OsString, path::PathBuf};

#[derive(Default)]
pub struct Options {
    pub working_directory: Option<PathBuf>,
    pub profile: Option<String>,
    pub new_window: bool,
    #[cfg(windows)]
    pub handoff: Option<crate::handoff::Incoming>,
    #[cfg(debug_assertions)]
    pub smoke_mode: Option<String>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            #[cfg(windows)]
            if let Some(value) = arg
                .to_str()
                .and_then(|s| s.strip_prefix("--terminal-handoff="))
            {
                if options.handoff.is_some() {
                    bail!(crate::i18n::tr(
                        "console_handoff_can_only_be_specified_once"
                    ));
                }
                options.handoff = Some(crate::handoff::Incoming::inherit(value)?);
                continue;
            }
            #[cfg(windows)]
            if let Some(title) = arg
                .to_str()
                .and_then(|s| s.strip_prefix("--handoff-title="))
            {
                options
                    .handoff
                    .as_mut()
                    .context(crate::i18n::tr("a_title_requires_a_console_handoff"))?
                    .title = title.into();
                continue;
            }
            if arg == "--new-window" {
                options.new_window = true;
                continue;
            }
            let profile = if arg == "--profile" || arg == "-p" {
                Some(
                    args.next()
                        .context(crate::i18n::tr("specify_a_profile_id_after_profile"))?,
                )
            } else {
                arg.to_str()
                    .and_then(|s| s.strip_prefix("--profile="))
                    .map(OsString::from)
            };
            if let Some(profile) = profile {
                let profile = profile.into_string().map_err(|_| {
                    anyhow::anyhow!(crate::i18n::tr("the_profile_id_must_be_valid_unicode"))
                })?;
                if profile.trim().is_empty() || profile.contains('\0') {
                    bail!(crate::i18n::tr("the_profile_id_is_empty_or_invalid"));
                }
                if options.profile.replace(profile).is_some() {
                    bail!(crate::i18n::tr("the_profile_can_only_be_specified_once"));
                }
                continue;
            }
            let directory = if arg == "--working-directory" || arg == "-d" {
                args.next().context(crate::i18n::tr(
                    "specify_a_folder_path_after_working_directory",
                ))?
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
                bail!(crate::tf!("unknown_launch_argument_arg0", arg0 => arg.to_string_lossy()));
            };
            if options.working_directory.is_some() {
                bail!(crate::i18n::tr(
                    "the_starting_directory_can_only_be_specified_once"
                ));
            }
            if directory.is_empty() {
                bail!(crate::i18n::tr("the_starting_directory_path_is_empty"));
            }
            let path = std::path::absolute(PathBuf::from(directory))?;
            if !path.is_dir() {
                bail!(crate::tf!("starting_directory_not_found_arg0", arg0 => path.display()));
            }
            options.working_directory = Some(path);
        }
        #[cfg(windows)]
        if options.handoff.is_some()
            && (options.profile.is_some() || options.working_directory.is_some())
        {
            bail!(crate::i18n::tr(
                "a_console_handoff_cannot_specify_a_profile_or_starting_directory"
            ));
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
    fn profile_ids_preserve_unicode_and_combine_with_launch_options() {
        let id = "wsl:개발 Ubuntu \\\"quoted\\\"";
        let options = parse(&["--profile", id, "-d", ".", "--new-window"]).unwrap();
        assert_eq!(options.profile.as_deref(), Some(id));
        assert!(options.new_window);
        assert!(options.working_directory.is_some());
        assert_eq!(
            parse(&["--profile=cmd"]).unwrap().profile.as_deref(),
            Some("cmd")
        );
        assert_eq!(
            parse(&["-p", "powershell"]).unwrap().profile.as_deref(),
            Some("powershell")
        );
        assert!(parse(&["--profile"]).is_err());
        assert!(parse(&["--profile="]).is_err());
        assert!(parse(&["-p", " "]).is_err());
        assert!(parse(&["-p", "a", "--profile=b"]).is_err());
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
