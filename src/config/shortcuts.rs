//! Supported Windows Terminal default actions. Unsupported actions stay unbound.
use anyhow::{Result, bail};
use gpui::{Keystroke, OperatingSystem, PlatformKeyboardMapper};
use std::collections::{BTreeMap, HashMap};

pub type Overrides = BTreeMap<String, Vec<String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewTab,
    NewWindow,
    CloseTab,
    CloseWindow,
    NextTab,
    PreviousTab,
    NewProfile(usize),
    SwitchTab(usize),
    LastTab,
    ProfileMenu,
    Settings,
    SettingsFile,
    Find,
    Copy,
    Paste,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ScrollTop,
    ScrollBottom,
    Fullscreen,
    IncreaseFont,
    DecreaseFont,
    ResetFont,
}

pub struct Definition {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub defaults: &'static [&'static str],
    pub action: Action,
}

macro_rules! binding {
    ($id:literal, $label:literal, $group:literal, $action:expr, $($key:literal),+ $(,)?) => {
        Definition { id: $id, label: $label, group: $group, defaults: &[$($key),+], action: $action }
    };
}

// Source: microsoft/terminal, TerminalSettingsModel/defaults.json, keybindings.
// GPUI spelling uses '-' separators and pageup/pagedown instead of pgup/pgdn.
pub const DEFINITIONS: &[Definition] = &[
    binding!("newTab", "새 탭", "탭과 창", Action::NewTab, "ctrl-shift-t"),
    binding!(
        "newWindow",
        "새 창",
        "탭과 창",
        Action::NewWindow,
        "ctrl-shift-n"
    ),
    binding!(
        "closeTab",
        "현재 탭 닫기",
        "탭과 창",
        Action::CloseTab,
        "ctrl-shift-w"
    ),
    binding!(
        "closeWindow",
        "창 닫기",
        "탭과 창",
        Action::CloseWindow,
        "alt-f4"
    ),
    binding!("nextTab", "다음 탭", "탭과 창", Action::NextTab, "ctrl-tab"),
    binding!(
        "prevTab",
        "이전 탭",
        "탭과 창",
        Action::PreviousTab,
        "ctrl-shift-tab"
    ),
    binding!(
        "switchToTab1",
        "첫 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(0),
        "ctrl-alt-1"
    ),
    binding!(
        "switchToTab2",
        "두 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(1),
        "ctrl-alt-2"
    ),
    binding!(
        "switchToTab3",
        "세 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(2),
        "ctrl-alt-3"
    ),
    binding!(
        "switchToTab4",
        "네 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(3),
        "ctrl-alt-4"
    ),
    binding!(
        "switchToTab5",
        "다섯 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(4),
        "ctrl-alt-5"
    ),
    binding!(
        "switchToTab6",
        "여섯 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(5),
        "ctrl-alt-6"
    ),
    binding!(
        "switchToTab7",
        "일곱 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(6),
        "ctrl-alt-7"
    ),
    binding!(
        "switchToTab8",
        "여덟 번째 탭",
        "탭 바로 가기",
        Action::SwitchTab(7),
        "ctrl-alt-8"
    ),
    binding!(
        "switchToLastTab",
        "마지막 탭",
        "탭 바로 가기",
        Action::LastTab,
        "ctrl-alt-9"
    ),
    binding!(
        "newTabProfile1",
        "프로필 1 실행",
        "프로필",
        Action::NewProfile(0),
        "ctrl-shift-1"
    ),
    binding!(
        "newTabProfile2",
        "프로필 2 실행",
        "프로필",
        Action::NewProfile(1),
        "ctrl-shift-2"
    ),
    binding!(
        "newTabProfile3",
        "프로필 3 실행",
        "프로필",
        Action::NewProfile(2),
        "ctrl-shift-3"
    ),
    binding!(
        "newTabProfile4",
        "프로필 4 실행",
        "프로필",
        Action::NewProfile(3),
        "ctrl-shift-4"
    ),
    binding!(
        "newTabProfile5",
        "프로필 5 실행",
        "프로필",
        Action::NewProfile(4),
        "ctrl-shift-5"
    ),
    binding!(
        "newTabProfile6",
        "프로필 6 실행",
        "프로필",
        Action::NewProfile(5),
        "ctrl-shift-6"
    ),
    binding!(
        "newTabProfile7",
        "프로필 7 실행",
        "프로필",
        Action::NewProfile(6),
        "ctrl-shift-7"
    ),
    binding!(
        "newTabProfile8",
        "프로필 8 실행",
        "프로필",
        Action::NewProfile(7),
        "ctrl-shift-8"
    ),
    binding!(
        "newTabProfile9",
        "프로필 9 실행",
        "프로필",
        Action::NewProfile(8),
        "ctrl-shift-9"
    ),
    binding!(
        "openNewTabDropdown",
        "프로필 메뉴",
        "설정과 찾기",
        Action::ProfileMenu,
        "ctrl-shift-space"
    ),
    binding!(
        "openSettings",
        "설정 열기",
        "설정과 찾기",
        Action::Settings,
        "ctrl-,"
    ),
    binding!(
        "openSettingsFile",
        "설정 파일 열기",
        "설정과 찾기",
        Action::SettingsFile,
        "ctrl-shift-,"
    ),
    binding!(
        "find",
        "터미널에서 찾기",
        "설정과 찾기",
        Action::Find,
        "ctrl-shift-f"
    ),
    binding!(
        "copy",
        "선택 영역 복사",
        "클립보드",
        Action::Copy,
        "ctrl-shift-c",
        "ctrl-insert",
        "enter"
    ),
    binding!(
        "paste",
        "붙여넣기",
        "클립보드",
        Action::Paste,
        "ctrl-shift-v",
        "shift-insert"
    ),
    binding!(
        "scrollUp",
        "위로 스크롤",
        "스크롤",
        Action::ScrollUp,
        "ctrl-shift-up"
    ),
    binding!(
        "scrollDown",
        "아래로 스크롤",
        "스크롤",
        Action::ScrollDown,
        "ctrl-shift-down"
    ),
    binding!(
        "scrollUpPage",
        "한 페이지 위로 스크롤",
        "스크롤",
        Action::PageUp,
        "ctrl-shift-pageup"
    ),
    binding!(
        "scrollDownPage",
        "한 페이지 아래로 스크롤",
        "스크롤",
        Action::PageDown,
        "ctrl-shift-pagedown"
    ),
    binding!(
        "scrollToTop",
        "맨 위로 스크롤",
        "스크롤",
        Action::ScrollTop,
        "ctrl-shift-home"
    ),
    binding!(
        "scrollToBottom",
        "맨 아래로 스크롤",
        "스크롤",
        Action::ScrollBottom,
        "ctrl-shift-end"
    ),
    binding!(
        "toggleFullscreen",
        "전체 화면 전환",
        "화면",
        Action::Fullscreen,
        "alt-enter",
        "f11"
    ),
    binding!(
        "increaseFontSize",
        "글꼴 크기 늘리기",
        "화면",
        Action::IncreaseFont,
        "ctrl-+"
    ),
    binding!(
        "decreaseFontSize",
        "글꼴 크기 줄이기",
        "화면",
        Action::DecreaseFont,
        "ctrl--"
    ),
    binding!(
        "resetFontSize",
        "글꼴 크기 초기화",
        "화면",
        Action::ResetFont,
        "ctrl-0"
    ),
];

pub fn keys(overrides: &Overrides, definition: &Definition) -> Vec<String> {
    overrides.get(definition.id).cloned().unwrap_or_else(|| {
        definition
            .defaults
            .iter()
            .map(|key| {
                if cfg!(target_os = "macos") {
                    key.replace("ctrl-", "cmd-")
                } else {
                    (*key).to_owned()
                }
            })
            .collect()
    })
}

pub fn parse(text: &str) -> Result<Keystroke> {
    let key = Keystroke::parse_with_os(text, OperatingSystem::Windows)?;
    let name = key.key.as_str();
    let valid = name.chars().count() == 1
        || matches!(
            name,
            "enter"
                | "tab"
                | "escape"
                | "space"
                | "backspace"
                | "insert"
                | "delete"
                | "up"
                | "down"
                | "left"
                | "right"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "menu"
        )
        || name
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
    if !valid || key.is_modifier_key() || key.key_char.is_some() {
        bail!("사용할 수 없는 단축키: {text}");
    }
    Ok(key)
}

pub fn label(text: &str) -> String {
    let Ok(key) = parse(text) else {
        return text.into();
    };
    let mut parts = Vec::new();
    if key.modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if key.modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if key.modifiers.shift {
        parts.push("Shift".to_owned());
    }
    if key.modifiers.platform {
        parts.push(
            if cfg!(target_os = "macos") {
                "Cmd"
            } else {
                "Win"
            }
            .to_owned(),
        );
    }
    if key.modifiers.function {
        parts.push("Fn".to_owned());
    }
    let name = match key.key.as_str() {
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        "escape" => "Esc".into(),
        "backspace" => "Backspace".into(),
        "space" => "Space".into(),
        "enter" => "Enter".into(),
        "tab" => "Tab".into(),
        "insert" => "Insert".into(),
        "delete" => "Delete".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        name => name.to_uppercase(),
    };
    parts.push(name);
    parts.join("+")
}

pub struct Bindings(Vec<(Keystroke, Action, &'static str)>);

impl Bindings {
    pub fn new(overrides: &Overrides) -> Result<Self> {
        for id in overrides.keys() {
            if !DEFINITIONS.iter().any(|d| d.id == id) {
                bail!("지원하지 않는 단축키 동작: {id}");
            }
        }
        let mut bindings = Vec::new();
        let mut used = HashMap::new();
        for definition in DEFINITIONS {
            for text in keys(overrides, definition) {
                let key = parse(&text)?;
                if let Some(previous) =
                    used.insert((key.modifiers, key.key.clone()), definition.label)
                {
                    bail!(
                        "{}: ‘{}’와 ‘{}’의 단축키가 겹칩니다.",
                        label(&text),
                        previous,
                        definition.label
                    );
                }
                bindings.push((key, definition.action, definition.label));
            }
        }
        Ok(Self(bindings))
    }

    pub fn action(&self, event: &Keystroke, mapper: &dyn PlatformKeyboardMapper) -> Option<Action> {
        let event = mapper.map_key_equivalent(event.clone(), false);
        self.0.iter().find_map(|(key, action, _)| {
            let target = mapper.map_key_equivalent(key.clone(), false);
            (event.inner().modifiers == target.inner().modifiers
                && event.inner().key == target.inner().key)
                .then_some(*action)
        })
    }

    pub fn validate_layout(&self, mapper: &dyn PlatformKeyboardMapper) -> Result<()> {
        let mut used = HashMap::new();
        for (key, _, action) in &self.0 {
            let mapped = mapper.map_key_equivalent(key.clone(), false);
            if let Some(previous) = used.insert(
                (mapped.inner().modifiers, mapped.inner().key.clone()),
                action,
            ) {
                bail!("현재 키보드 배열에서 ‘{previous}’와 ‘{action}’의 단축키가 겹칩니다.");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::DummyKeyboardMapper;

    #[test]
    fn windows_defaults_and_context_sensitive_enter() {
        let bindings = Bindings::new(&Overrides::new()).unwrap();
        for (key, expected) in [
            ("ctrl-shift-space", Some(Action::ProfileMenu)),
            ("ctrl-shift-p", None),
            ("ctrl-alt-9", Some(Action::LastTab)),
            ("ctrl-insert", Some(Action::Copy)),
            ("enter", Some(Action::Copy)),
            ("shift-enter", None),
            ("ctrl-enter", None),
            ("alt-enter", Some(Action::Fullscreen)),
            ("ctrl-shift-,", Some(Action::SettingsFile)),
        ] {
            assert_eq!(
                bindings.action(&parse(key).unwrap(), &DummyKeyboardMapper),
                expected,
                "{key}"
            );
        }
    }

    #[test]
    fn overrides_replace_disable_and_reject_conflicts() {
        let mut overrides = Overrides::from([
            ("newTab".into(), vec!["ctrl-t".into()]),
            ("copy".into(), vec![]),
        ]);
        let bindings = Bindings::new(&overrides).unwrap();
        assert_eq!(
            bindings.action(&parse("ctrl-t").unwrap(), &DummyKeyboardMapper),
            Some(Action::NewTab)
        );
        assert_eq!(
            bindings.action(&parse("ctrl-shift-t").unwrap(), &DummyKeyboardMapper),
            None
        );
        assert_eq!(
            bindings.action(&parse("enter").unwrap(), &DummyKeyboardMapper),
            None
        );
        overrides.insert("paste".into(), vec!["ctrl-t".into()]);
        assert!(Bindings::new(&overrides).is_err());
        assert!(Bindings::new(&Overrides::from([("splitPane".into(), vec![])])).is_err());
        assert!(parse("ctrl-nonsense").is_err());
        assert!(parse("ctrl").is_err());
    }
}
