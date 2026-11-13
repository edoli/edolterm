/// The Windows launcher initializes the console title with the profile name.
/// Preserve application title changes, including localized elevation prefixes.
pub(super) fn display(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_titles_are_preserved_without_prefix_or_path_parsing() {
        for title in [
            "PowerShell",
            "관리자: PowerShell",
            "Administrator: PowerShell",
            "개발 서버",
            r"관리자: C:\custom\pwsh.exe",
            "",
        ] {
            assert_eq!(display(title), title);
        }
    }

    #[test]
    fn titles_are_sanitized_and_bounded_in_characters() {
        assert_eq!(display("a\n\u{1b}b"), "ab");
        assert_eq!(display(&"한".repeat(300)), "한".repeat(256));
    }
}
