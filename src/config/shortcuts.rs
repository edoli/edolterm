//! Supported Windows Terminal default actions. Unsupported actions stay unbound.
use anyhow::{Result, bail};
use gpui::{Keystroke, OperatingSystem, PlatformKeyboardMapper};
use std::collections::{BTreeMap, HashMap};

pub type Overrides = BTreeMap<String, Vec<String>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewTab,
    DuplicateTab,
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
    SelectAll,
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
    /// Translation IDs, resolved when rendering or reporting a conflict.
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
    binding!(
        "newTab",
        "new_tab",
        "tabs_and_windows",
        Action::NewTab,
        "ctrl-shift-t"
    ),
    binding!(
        "duplicateTab",
        "duplicate_tab",
        "tabs_and_windows",
        Action::DuplicateTab,
        "ctrl-shift-d"
    ),
    binding!(
        "newWindow",
        "new_window",
        "tabs_and_windows",
        Action::NewWindow,
        "ctrl-shift-n"
    ),
    binding!(
        "closeTab",
        "close_current_tab",
        "tabs_and_windows",
        Action::CloseTab,
        "ctrl-shift-w"
    ),
    binding!(
        "closeWindow",
        "close_window",
        "tabs_and_windows",
        Action::CloseWindow,
        "alt-f4"
    ),
    binding!(
        "nextTab",
        "next_tab",
        "tabs_and_windows",
        Action::NextTab,
        "ctrl-tab"
    ),
    binding!(
        "prevTab",
        "previous_tab",
        "tabs_and_windows",
        Action::PreviousTab,
        "ctrl-shift-tab"
    ),
    binding!(
        "switchToTab1",
        "first_tab",
        "switch_to_tab",
        Action::SwitchTab(0),
        "ctrl-alt-1"
    ),
    binding!(
        "switchToTab2",
        "second_tab",
        "switch_to_tab",
        Action::SwitchTab(1),
        "ctrl-alt-2"
    ),
    binding!(
        "switchToTab3",
        "third_tab",
        "switch_to_tab",
        Action::SwitchTab(2),
        "ctrl-alt-3"
    ),
    binding!(
        "switchToTab4",
        "fourth_tab",
        "switch_to_tab",
        Action::SwitchTab(3),
        "ctrl-alt-4"
    ),
    binding!(
        "switchToTab5",
        "fifth_tab",
        "switch_to_tab",
        Action::SwitchTab(4),
        "ctrl-alt-5"
    ),
    binding!(
        "switchToTab6",
        "sixth_tab",
        "switch_to_tab",
        Action::SwitchTab(5),
        "ctrl-alt-6"
    ),
    binding!(
        "switchToTab7",
        "seventh_tab",
        "switch_to_tab",
        Action::SwitchTab(6),
        "ctrl-alt-7"
    ),
    binding!(
        "switchToTab8",
        "eighth_tab",
        "switch_to_tab",
        Action::SwitchTab(7),
        "ctrl-alt-8"
    ),
    binding!(
        "switchToLastTab",
        "last_tab",
        "switch_to_tab",
        Action::LastTab,
        "ctrl-alt-9"
    ),
    binding!(
        "newTabProfile1",
        "launch_profile_1",
        "profiles",
        Action::NewProfile(0),
        "ctrl-shift-1"
    ),
    binding!(
        "newTabProfile2",
        "launch_profile_2",
        "profiles",
        Action::NewProfile(1),
        "ctrl-shift-2"
    ),
    binding!(
        "newTabProfile3",
        "launch_profile_3",
        "profiles",
        Action::NewProfile(2),
        "ctrl-shift-3"
    ),
    binding!(
        "newTabProfile4",
        "launch_profile_4",
        "profiles",
        Action::NewProfile(3),
        "ctrl-shift-4"
    ),
    binding!(
        "newTabProfile5",
        "launch_profile_5",
        "profiles",
        Action::NewProfile(4),
        "ctrl-shift-5"
    ),
    binding!(
        "newTabProfile6",
        "launch_profile_6",
        "profiles",
        Action::NewProfile(5),
        "ctrl-shift-6"
    ),
    binding!(
        "newTabProfile7",
        "launch_profile_7",
        "profiles",
        Action::NewProfile(6),
        "ctrl-shift-7"
    ),
    binding!(
        "newTabProfile8",
        "launch_profile_8",
        "profiles",
        Action::NewProfile(7),
        "ctrl-shift-8"
    ),
    binding!(
        "newTabProfile9",
        "launch_profile_9",
        "profiles",
        Action::NewProfile(8),
        "ctrl-shift-9"
    ),
    binding!(
        "openNewTabDropdown",
        "profile_menu",
        "settings_and_search",
        Action::ProfileMenu,
        "ctrl-shift-space"
    ),
    binding!(
        "openSettings",
        "open_settings",
        "settings_and_search",
        Action::Settings,
        "ctrl-,"
    ),
    binding!(
        "openSettingsFile",
        "open_settings_file",
        "settings_and_search",
        Action::SettingsFile,
        "ctrl-shift-,"
    ),
    binding!(
        "find",
        "find_in_terminal",
        "settings_and_search",
        Action::Find,
        "ctrl-shift-f"
    ),
    binding!(
        "copy",
        "copy_selection",
        "clipboard",
        Action::Copy,
        "ctrl-shift-c",
        "ctrl-insert",
        "enter"
    ),
    binding!(
        "selectAll",
        "select_all",
        "clipboard",
        Action::SelectAll,
        "ctrl-shift-a"
    ),
    binding!(
        "paste",
        "paste",
        "clipboard",
        Action::Paste,
        "ctrl-shift-v",
        "shift-insert"
    ),
    binding!(
        "scrollUp",
        "scroll_up",
        "scrolling",
        Action::ScrollUp,
        "ctrl-shift-up"
    ),
    binding!(
        "scrollDown",
        "scroll_down",
        "scrolling",
        Action::ScrollDown,
        "ctrl-shift-down"
    ),
    binding!(
        "scrollUpPage",
        "scroll_up_one_page",
        "scrolling",
        Action::PageUp,
        "ctrl-shift-pageup"
    ),
    binding!(
        "scrollDownPage",
        "scroll_down_one_page",
        "scrolling",
        Action::PageDown,
        "ctrl-shift-pagedown"
    ),
    binding!(
        "scrollToTop",
        "scroll_to_top",
        "scrolling",
        Action::ScrollTop,
        "ctrl-shift-home"
    ),
    binding!(
        "scrollToBottom",
        "scroll_to_bottom",
        "scrolling",
        Action::ScrollBottom,
        "ctrl-shift-end"
    ),
    binding!(
        "toggleFullscreen",
        "toggle_full_screen",
        "display",
        Action::Fullscreen,
        "alt-enter",
        "f11"
    ),
    binding!(
        "increaseFontSize",
        "increase_font_size",
        "display",
        Action::IncreaseFont,
        "ctrl-=",
        "ctrl-numpad_plus"
    ),
    binding!(
        "decreaseFontSize",
        "decrease_font_size",
        "display",
        Action::DecreaseFont,
        "ctrl--",
        "ctrl-numpad_minus"
    ),
    binding!(
        "resetFontSize",
        "reset_font_size",
        "display",
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
                | "numpad_plus"
                | "numpad_minus"
        )
        || name
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
    if !valid || key.is_modifier_key() || key.key_char.is_some() {
        bail!(crate::tf!("invalid_shortcut_text", text => text));
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
        "numpad_plus" => "Numpad +".into(),
        "numpad_minus" => "Numpad -".into(),
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
                bail!(crate::tf!("unsupported_shortcut_action_id", id => id));
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
                        crate::tf!("arg0_shortcuts_for_arg1_and_arg2_overlap", arg0 => label(&text), arg1 => crate::i18n::tr(previous), arg2 => crate::i18n::tr(definition.label))
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
                bail!(
                    crate::tf!("shortcuts_for_previous_and_action_overlap_in_the_current_keyboard_layo", previous => crate::i18n::tr(previous), action => crate::i18n::tr(action))
                );
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
            ("ctrl-shift-d", Some(Action::DuplicateTab)),
            ("ctrl-shift-a", Some(Action::SelectAll)),
            ("ctrl-shift-k", None),
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

    #[test]
    fn font_shortcuts_distinguish_main_keyboard_and_keypad() {
        let bindings = Bindings::new(&Overrides::new()).unwrap();
        for (key, expected) in [
            ("ctrl-=", Some(Action::IncreaseFont)),
            ("ctrl-numpad_plus", Some(Action::IncreaseFont)),
            ("ctrl--", Some(Action::DecreaseFont)),
            ("ctrl-numpad_minus", Some(Action::DecreaseFont)),
            ("ctrl-+", None),
            ("ctrl-shift-=", None),
            ("ctrl-shift-numpad_plus", None),
            ("ctrl-shift-numpad_minus", None),
        ] {
            assert_eq!(
                bindings.action(&parse(key).unwrap(), &DummyKeyboardMapper),
                expected,
                "{key}"
            );
        }
        assert_eq!(label("ctrl-numpad_plus"), "Ctrl+Numpad +");
        assert_eq!(label("ctrl-numpad_minus"), "Ctrl+Numpad -");

        let overrides = Overrides::from([
            ("increaseFontSize".into(), vec!["ctrl-numpad_plus".into()]),
            ("decreaseFontSize".into(), vec!["ctrl-numpad_minus".into()]),
            ("newTab".into(), vec!["ctrl-=".into(), "ctrl--".into()]),
        ]);
        let restored = toml::from_str(&toml::to_string(&overrides).unwrap()).unwrap();
        let bindings = Bindings::new(&restored).unwrap();
        bindings.validate_layout(&DummyKeyboardMapper).unwrap();
        for (key, expected) in [
            ("ctrl-=", Action::NewTab),
            ("ctrl--", Action::NewTab),
            ("ctrl-numpad_plus", Action::IncreaseFont),
            ("ctrl-numpad_minus", Action::DecreaseFont),
        ] {
            assert_eq!(
                bindings.action(&parse(key).unwrap(), &DummyKeyboardMapper),
                Some(expected),
                "{key}"
            );
        }
    }
}
