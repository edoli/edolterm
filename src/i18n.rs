//! Embedded message catalogs and application-wide UI language selection.
//! Keep message IDs stable; add languages to LOCALES without changing UI code.
use std::{
    collections::BTreeMap,
    sync::{LazyLock, RwLock},
};

type Messages = BTreeMap<String, String>;

pub struct Locale {
    pub code: &'static str,
    pub native_name: &'static str,
    messages: &'static LazyLock<Messages>,
}

static ENGLISH: LazyLock<Messages> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../locales/en.json")).expect("valid English catalog")
});
static KOREAN: LazyLock<Messages> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../locales/ko.json")).expect("valid Korean catalog")
});

pub static LOCALES: &[Locale] = &[
    Locale {
        code: "en",
        native_name: "English",
        messages: &ENGLISH,
    },
    Locale {
        code: "ko",
        native_name: "한국어",
        messages: &KOREAN,
    },
];

// A single process owns all windows. Worker-thread errors use the same language.
static CURRENT: RwLock<&'static Locale> = RwLock::new(&LOCALES[0]);
static SYSTEM: LazyLock<&'static Locale> =
    LazyLock::new(|| locale_for_tag(&system_language().unwrap_or_default()));

pub fn locale_for_tag(tag: &str) -> &'static Locale {
    let normalized = tag
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    let mut candidate = normalized.as_str();
    loop {
        if let Some(locale) = LOCALES
            .iter()
            .find(|locale| locale.code.eq_ignore_ascii_case(candidate))
        {
            return locale;
        }
        let Some((parent, _)) = candidate.rsplit_once('-') else {
            return &LOCALES[0];
        };
        candidate = parent;
    }
}

pub fn set_language(preference: &str) {
    let locale = if preference == "system" {
        *SYSTEM
    } else {
        locale_for_tag(preference)
    };
    *CURRENT.write().expect("language lock") = locale;
}

pub fn tr(id: &'static str) -> &'static str {
    text_in(*CURRENT.read().expect("language lock"), id)
}

fn text_in(locale: &'static Locale, id: &'static str) -> &'static str {
    locale
        .messages
        .get(id)
        .or_else(|| ENGLISH.get(id))
        .map(String::as_str)
        .unwrap_or(id)
}

/// Substitute named placeholders in one pass: inserted user text is never parsed.
pub fn format(id: &'static str, arguments: &[(&str, String)]) -> String {
    interpolate(tr(id), arguments)
}

fn interpolate(template: &str, arguments: &[(&str, String)]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}').map(|end| start + end) else {
            break;
        };
        result.push_str(&rest[..start]);
        let name = &rest[start + 1..end];
        if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
            result.push_str(value);
        } else {
            result.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

#[macro_export]
macro_rules! tf {
    ($id:literal, $($name:ident => $value:expr),+ $(,)?) => {
        $crate::i18n::format($id, &[$((stringify!($name), format!("{}", $value))),+])
    };
}

#[cfg(windows)]
fn system_language() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
    let mut name = [0u16; 85];
    // UI language, rather than the region or keyboard layout.
    let length = unsafe {
        LCIDToLocaleName(
            GetUserDefaultUILanguage() as u32,
            name.as_mut_ptr(),
            name.len() as i32,
            0,
        )
    };
    (length > 1).then(|| String::from_utf16_lossy(&name[..length as usize - 1]))
}

#[cfg(not(windows))]
fn system_language() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_language_variants_and_falls_back_to_english() {
        for tag in ["ko", "ko-KR", "KO-kr", "ko_KR.UTF-8"] {
            assert_eq!(locale_for_tag(tag).code, "ko");
        }
        for tag in ["en-US", "en-GB", "fr-FR", "ja-JP", "", "invalid"] {
            assert_eq!(locale_for_tag(tag).code, "en");
        }
    }

    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .map(|part| part.split_once('}').expect("closed placeholder").0)
            .collect()
    }

    #[test]
    fn catalogs_have_the_same_messages_and_placeholders() {
        for locale in LOCALES {
            assert_eq!(
                ENGLISH.keys().collect::<Vec<_>>(),
                locale.messages.keys().collect::<Vec<_>>(),
                "{}",
                locale.code
            );
            for (id, english) in ENGLISH.iter() {
                let translated = &locale.messages[id];
                assert!(!translated.trim().is_empty(), "{}: {id}", locale.code);
                assert_eq!(
                    placeholders(english),
                    placeholders(translated),
                    "{}: {id}",
                    locale.code
                );
            }
        }
        assert_eq!(text_in(locale_for_tag("en"), "settings"), "Settings");
        assert_eq!(text_in(locale_for_tag("ko"), "settings"), "설정");
        assert_eq!(
            text_in(locale_for_tag("ko"), "unknown.message"),
            "unknown.message"
        );
    }

    #[test]
    fn substitution_can_reorder_arguments_and_preserves_user_text() {
        assert_eq!(
            interpolate(
                "{b} / {a} / {b}",
                &[("a", "{b} 🙂".into()), ("b", "hello".into())]
            ),
            "hello / {b} 🙂 / hello"
        );
    }

    #[test]
    fn incomplete_catalog_falls_back_per_message() {
        static EMPTY: LazyLock<Messages> = LazyLock::new(Messages::new);
        static LOCALE: Locale = Locale {
            code: "test",
            native_name: "Test",
            messages: &EMPTY,
        };
        assert_eq!(text_in(&LOCALE, "settings"), "Settings");
    }
}
