use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[path = "color_schemes.rs"]
pub mod color_schemes;

#[path = "config/shortcuts.rs"]
pub mod shortcuts;

pub struct SharedSettings(pub Settings);
impl gpui::Global for SharedSettings {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// BCP-47 language code, or "system" to follow the Windows display language.
    pub language: String,
    pub default_profile: String,
    pub font_family: String,
    pub font_size: f32,
    /// Terminal row height as a multiple of the configured font size.
    pub line_height: f32,
    /// Terminal column width as a multiple of the configured font size.
    pub cell_width: f32,
    pub ctrl_scroll_zoom: bool,
    pub detect_urls: bool,
    pub scrollback: usize,
    pub theme: String,
    pub color_scheme: String,
    pub confirm_close: bool,
    pub restore_session: bool,
    pub allow_osc52: bool,
    pub windowing_behavior: WindowingBehavior,
    pub profiles: Vec<Profile>,
    /// Saved results of the first-run search or a refresh in preferences.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub discovered_profiles: Vec<Profile>,
    /// Per-action overrides. An empty list disables that action's shortcuts.
    pub keybindings: BTreeMap<String, Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            default_profile: String::new(),
            font_family: if cfg!(windows) {
                "Cascadia Mono"
            } else {
                "monospace"
            }
            .into(),
            font_size: 15.,
            line_height: 1.2,
            cell_width: 0.6,
            ctrl_scroll_zoom: true,
            detect_urls: true,
            scrollback: 10_000,
            theme: "system".into(),
            color_scheme: crate::config::color_schemes::DEFAULT_SCHEME.into(),
            confirm_close: true,
            restore_session: true,
            allow_osc52: false,
            windowing_behavior: WindowingBehavior::CurrentDesktop,
            profiles: Vec::new(),
            discovered_profiles: Vec::new(),
            keybindings: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum WindowingBehavior {
    #[serde(rename = "useNew")]
    NewWindow,
    #[default]
    #[serde(rename = "useExisting")]
    CurrentDesktop,
    #[serde(rename = "useAnyExisting")]
    AnyDesktop,
}

impl WindowingBehavior {
    pub const ALL: [Self; 3] = [Self::NewWindow, Self::AnyDesktop, Self::CurrentDesktop];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewWindow => crate::i18n::tr("create_a_new_window"),
            Self::AnyDesktop => crate::i18n::tr("attach_to_the_most_recently_used_window"),
            Self::CurrentDesktop => {
                crate::i18n::tr("attach_to_the_most_recently_used_window_on_this_desktop")
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Profile {
    pub fn new(id: &str, name: &str, program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            program: program.into(),
            args: args.iter().map(|s| (*s).into()).collect(),
            cwd: None,
            env: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        shortcuts::Bindings::new(&self.keybindings)?;
        if !self.font_size.is_finite() || !(8. ..=48.).contains(&self.font_size) {
            bail!(crate::i18n::tr("font_size_must_be_between_8_and_48"));
        }
        if !self.line_height.is_finite() || !(0.5..=3.).contains(&self.line_height) {
            bail!(crate::i18n::tr("line_height_must_be_between_0_5_and_3_0"));
        }
        if !self.cell_width.is_finite() || !(0.3..=2.).contains(&self.cell_width) {
            bail!(crate::i18n::tr("cell_width_must_be_between_0_3_and_2_0"));
        }
        if self.font_family.trim().is_empty() {
            bail!(crate::i18n::tr("enter_a_font_family_name"));
        }
        if self.scrollback > 1_000_000 {
            bail!(crate::i18n::tr("scrollback_cannot_exceed_1_000_000_lines"));
        }
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            bail!(crate::i18n::tr("theme_must_be_dark_light_or_system"));
        }
        if crate::config::color_schemes::find(&self.color_scheme).is_none() {
            bail!(crate::tf!("unknown_color_scheme_arg0", arg0 => self.color_scheme));
        }
        // Validate each list independently: user profiles may override discoveries.
        for profiles in [&self.profiles, &self.discovered_profiles] {
            let mut ids = std::collections::HashSet::new();
            for p in profiles {
                if p.id.trim().is_empty() || p.name.trim().is_empty() || p.program.trim().is_empty()
                {
                    bail!(crate::i18n::tr(
                        "profile_id_name_and_program_cannot_be_empty"
                    ));
                }
                if !ids.insert(&p.id) {
                    bail!(crate::tf!("duplicate_profile_id_arg0", arg0 => p.id));
                }
                if p.program.contains('\0') || p.args.iter().any(|s| s.contains('\0')) {
                    bail!(crate::i18n::tr(
                        "the_program_and_arguments_cannot_contain_nul_characters"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("settings.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let settings: Self = toml::from_str(
            &std::fs::read_to_string(&path)
                .with_context(|| crate::tf!("failed_to_read_arg0", arg0 => path.display()))?,
        )
        .context(crate::i18n::tr("invalid_settings_toml_format"))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(
            &dir.join("settings.toml"),
            toml::to_string_pretty(self)?.as_bytes(),
        )
    }
}

pub fn directory() -> PathBuf {
    if let Some(p) = std::env::var_os("EDOLTERM_CONFIG_DIR") {
        return p.into();
    }
    #[cfg(windows)]
    if let Some(p) = std::env::var_os("APPDATA") {
        return PathBuf::from(p).join("edolterm");
    }
    #[cfg(target_os = "macos")]
    if let Some(p) = std::env::var_os("HOME") {
        return PathBuf::from(p).join("Library/Application Support/edolterm");
    }
    if let Some(p) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(p).join("edolterm");
    }
    crate::platform::home_dir().join(".config/edolterm")
}

/// Replace in the same directory, including existing files on Windows.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .context(crate::i18n::tr("the_save_path_has_no_parent_directory"))?;
    std::fs::create_dir_all(parent)?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        crate::platform::replace_file(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result.with_context(|| crate::tf!("failed_to_save_arg0", arg0 => path.display()))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSession {
    pub tabs: Vec<SavedTab>,
    pub active: usize,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    /// Per-window snapshots have an empty `windows` array; old files remain readable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<SavedSession>,
    pub active_window: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTab {
    pub profile_id: String,
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

impl SavedSession {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("session.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn save(&self, dir: &Path) -> Result<()> {
        atomic_write(&dir.join("session.json"), &serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_defaults_for_legacy_settings_and_round_trips() {
        let legacy: Settings = toml::from_str("font_size = 14.0").unwrap();
        assert_eq!(legacy.language, "system");
        for language in ["system", "en", "ko", "fr-FR"] {
            let settings = Settings {
                language: language.into(),
                ..Settings::default()
            };
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(parsed.language, language);
            parsed.validate().unwrap();
        }
    }
    #[test]
    fn color_schemes_default_for_legacy_settings_and_persist() {
        let legacy: Settings = toml::from_str("theme = 'light'").unwrap();
        assert_eq!(legacy.color_scheme, "Campbell");
        for scheme in crate::config::color_schemes::COLOR_SCHEMES {
            let settings = Settings {
                color_scheme: scheme.name.into(),
                ..Settings::default()
            };
            settings.validate().unwrap();
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(settings, parsed);
        }
        assert!(
            Settings {
                color_scheme: "unknown".into(),
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn interaction_settings_default_on_and_round_trip_when_disabled() {
        let legacy: Settings = toml::from_str("font_size = 14.0").unwrap();
        assert!(legacy.ctrl_scroll_zoom);
        assert!(legacy.detect_urls);
        let settings = Settings {
            ctrl_scroll_zoom: false,
            detect_urls: false,
            ..Settings::default()
        };
        let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert_eq!(settings, parsed);
    }
    #[test]
    fn discoveries_round_trip_and_user_overrides_are_allowed() {
        let legacy: Settings = toml::from_str("font_size = 14.0").unwrap();
        assert!(legacy.discovered_profiles.is_empty());
        let cached = Profile::new("pwsh", "Detected", "pwsh.exe", &["-NoLogo"]);
        let mut settings = Settings {
            discovered_profiles: vec![cached.clone()],
            profiles: vec![Profile::new("pwsh", "Custom", "other.exe", &[])],
            ..Settings::default()
        };
        settings.validate().unwrap();
        let loaded: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded, settings);
        settings.discovered_profiles.push(cached);
        assert!(settings.validate().is_err());
    }
    #[test]
    fn saved_tab_titles_and_colors_are_optional_and_round_trip() {
        let legacy = r#"{"tabs":[{"profile_id":"shell","cwd":"."}]}"#;
        let mut saved: SavedSession = serde_json::from_str(legacy).unwrap();
        assert_eq!(saved.tabs[0].custom_title, None);
        assert_eq!(saved.tabs[0].color, None);
        assert!(
            !serde_json::to_string(&saved)
                .unwrap()
                .contains("custom_title")
        );
        saved.tabs[0].custom_title = Some("서버 🙂".into());
        saved.tabs[0].color = Some(0xe74856);
        let restored: SavedSession =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(restored.tabs[0].custom_title.as_deref(), Some("서버 🙂"));
        assert_eq!(restored.tabs[0].color, Some(0xe74856));
    }

    #[test]
    fn legacy_settings_default_to_current_desktop_and_all_modes_round_trip() {
        let legacy: Settings = toml::from_str("font_size = 14.0").unwrap();
        assert_eq!(legacy.windowing_behavior, WindowingBehavior::CurrentDesktop);
        assert_eq!(legacy.line_height, 1.2);
        assert_eq!(legacy.cell_width, 0.6);
        for mode in WindowingBehavior::ALL {
            let settings = Settings {
                windowing_behavior: mode,
                ..Settings::default()
            };
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(settings, parsed);
        }
        assert!(toml::from_str::<Settings>("windowing_behavior = 'unknown'").is_err());
    }

    #[test]
    fn line_height_defaults_round_trips_and_rejects_invalid_multipliers() {
        assert_eq!(Settings::default().line_height, 1.2);
        for line_height in [0.5, 1., 1.2, 1.5, 3.] {
            let settings = Settings {
                line_height,
                ..Settings::default()
            };
            settings.validate().unwrap();
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(parsed.line_height, line_height);
        }
        for line_height in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.,
            0.,
            0.49,
            3.01,
        ] {
            assert!(
                Settings {
                    line_height,
                    ..Settings::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn cell_width_defaults_round_trips_and_rejects_invalid_multipliers() {
        assert_eq!(Settings::default().cell_width, 0.6);
        for cell_width in [0.3, 0.6, 0.65, 1., 2.] {
            let settings = Settings {
                cell_width,
                ..Settings::default()
            };
            settings.validate().unwrap();
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(parsed.cell_width, cell_width);
        }
        for cell_width in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.,
            0.,
            0.29,
            2.01,
        ] {
            assert!(
                Settings {
                    cell_width,
                    ..Settings::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn legacy_sessions_and_multiple_windows_round_trip() {
        let legacy: SavedSession =
            serde_json::from_str(r#"{"tabs":[],"width":1200,"height":800,"active":0}"#).unwrap();
        assert!(legacy.windows.is_empty());
        let session = SavedSession {
            windows: vec![
                legacy.clone(),
                SavedSession {
                    width: 900.,
                    ..legacy.clone()
                },
            ],
            active_window: 1,
            ..legacy
        };
        let parsed: SavedSession =
            serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
        assert_eq!(parsed.windows.len(), 2);
        assert_eq!(parsed.windows[1].width, 900.);
        assert_eq!(parsed.active_window, 1);
        assert!(
            parsed
                .windows
                .iter()
                .all(|window| window.windows.is_empty())
        );
    }
    #[test]
    fn settings_round_trip_and_validation() {
        for theme in ["dark", "light", "system"] {
            let settings = Settings {
                theme: theme.into(),
                ..Settings::default()
            };
            settings.validate().unwrap();
            let parsed: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
            assert_eq!(settings, parsed);
        }
        assert!(
            Settings {
                theme: "unknown".into(),
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
        let mut settings = Settings::default();
        let mut profile = Profile::new(
            "custom",
            "개발 셸",
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            &["-NoLogo", "a b", "한글"],
        );
        profile
            .env
            .insert("TEST".into(), "value with spaces".into());
        settings.profiles.push(profile);
        let parsed: Settings = toml::from_str(&toml::to_string_pretty(&settings).unwrap()).unwrap();
        assert_eq!(settings, parsed);
        settings.profiles.push(settings.profiles[0].clone());
        assert!(settings.validate().is_err());
        settings.profiles.clear();
        settings.font_size = f32::NAN;
        assert!(settings.validate().is_err());
    }
    #[test]
    fn atomic_replace_and_invalid_settings_preservation() {
        let dir = std::env::temp_dir().join(format!("edolterm-config-test-{}", std::process::id()));
        let path = dir.join("settings.toml");
        Settings::default().save(&dir).unwrap();
        let settings = Settings {
            font_size: 18.,
            ..Settings::default()
        };
        settings.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap().font_size, 18.);
        atomic_write(&path, b"this is invalid TOML = [").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(Settings::load(&dir).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
