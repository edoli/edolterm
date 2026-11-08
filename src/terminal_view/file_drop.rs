//! External drops are pasted as shell arguments, without submitting the command.
use crate::config::Profile;
use std::path::PathBuf;

pub(super) fn paths_text(paths: &[PathBuf], profile: &Profile) -> String {
    let program = profile
        .program
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let powershell = matches!(
        program.as_str(),
        "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe"
    );
    let cmd = matches!(program.as_str(), "cmd" | "cmd.exe");
    let wsl = matches!(program.as_str(), "wsl" | "wsl.exe");
    let posix = wsl
        || !cfg!(windows)
        || matches!(
            program.as_str(),
            "sh" | "sh.exe" | "bash" | "bash.exe" | "zsh" | "zsh.exe" | "fish" | "fish.exe"
        );
    let mut text = String::new();
    for path in paths {
        let Some(path) = path.to_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        // Control characters in filenames must not become terminal input or
        // submit a command in shells without bracketed paste.
        if path.chars().any(char::is_control) {
            continue;
        }
        let path = if wsl {
            wsl_path(path)
        } else if posix && cfg!(windows) {
            path.replace('\\', "/")
        } else {
            path.to_owned()
        };
        if !text.is_empty() {
            text.push(' ');
        }
        if powershell {
            text.push('\'');
            text.push_str(&path.replace('\'', "''"));
            text.push('\'');
        } else if cmd || !posix {
            // Double quotes protect whitespace and cmd metacharacters.
            text.push('"');
            text.push_str(&path);
            text.push('"');
        } else {
            text.push('\'');
            text.push_str(&path.replace('\'', "'\\''"));
            text.push('\'');
        }
    }
    text
}

fn wsl_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    // Files dragged out of a WSL distribution already have a Linux path.
    for prefix in ["//wsl.localhost/", "//wsl$/"] {
        if path
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
            && let Some((_, rest)) = path[prefix.len()..].split_once('/')
        {
            return format!("/{rest}");
        }
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/" {
        return format!(
            "/mnt/{}{rest}",
            (bytes[0] as char).to_ascii_lowercase(),
            rest = &path[2..]
        );
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(program: &str, paths: &[&str]) -> String {
        paths_text(
            &paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
            &Profile::new("custom", "Shell", program, &[]),
        )
    }

    #[test]
    fn powershell_preserves_unicode_quotes_and_metacharacters() {
        assert_eq!(
            text(
                r#""C:\Program Files\PowerShell\7\pwsh.exe""#,
                &[r"C:\한글 폴더\it's $a; & [1].png", r"D:\second.txt"]
            ),
            "'C:\\한글 폴더\\it''s $a; & [1].png' 'D:\\second.txt'"
        );
    }

    #[test]
    fn cmd_quotes_spaces_and_operators() {
        assert_eq!(
            text("cmd.exe", &[r"C:\a & b\한글.txt", r"C:\two.txt"]),
            "\"C:\\a & b\\한글.txt\" \"C:\\two.txt\""
        );
    }

    #[test]
    fn posix_quotes_are_literal() {
        assert_eq!(
            text("/bin/bash", &["/tmp/it's $(id).png", "/tmp/한글 폴더"]),
            "'/tmp/it'\\''s $(id).png' '/tmp/한글 폴더'"
        );
    }

    #[test]
    fn wsl_translates_drive_and_distribution_paths() {
        assert_eq!(
            text(
                "wsl.exe",
                &[
                    r"C:\Users\한글 폴더\image.png",
                    r"\\wsl.localhost\Ubuntu\home\user\it's.txt",
                    r"\\wsl$\Ubuntu\tmp\file"
                ]
            ),
            "'/mnt/c/Users/한글 폴더/image.png' '/home/user/it'\\''s.txt' '/tmp/file'"
        );
    }

    #[test]
    fn empty_and_control_character_paths_cannot_submit_input() {
        assert_eq!(
            text(
                "pwsh",
                &["", "/tmp/a\rb", "/tmp/a\nb", "/tmp/a\x1bb", "/tmp/ok"]
            ),
            "'/tmp/ok'"
        );
        assert!(text("pwsh", &[]).is_empty());
    }
}
