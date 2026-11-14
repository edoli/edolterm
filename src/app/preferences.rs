//! Settings pages, shared navigation/footer, and preference editing controls.
use super::*;
use crate::config::color_schemes::{COLOR_SCHEMES, ColorScheme};
use crate::select::{Choice, Select, Selected};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPage {
    Behavior,
    Appearance,
    ColorSchemes,
    Profiles,
    Shortcuts,
}

impl SettingsPage {
    fn label(self) -> &'static str {
        match self {
            Self::Behavior => crate::i18n::tr("behavior"),
            Self::Appearance => crate::i18n::tr("appearance"),
            Self::ColorSchemes => crate::i18n::tr("color_schemes"),
            Self::Profiles => crate::i18n::tr("profiles"),
            Self::Shortcuts => crate::i18n::tr("keyboard_shortcuts"),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ColorSchemes => crate::i18n::tr(
                "choose_the_terminal_background_text_and_ansi_colors_save_to_apply_to_a",
            ),
            Self::Shortcuts => crate::i18n::tr(
                "uses_windows_terminal_default_shortcuts_only_implemented_actions_are_s",
            ),
            _ => crate::i18n::tr("save_settings_to_apply_them_to_open_terminals"),
        }
    }
}

fn settings_page_header(page: SettingsPage, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .mb_4()
        .child(
            div()
                .mb_1()
                .text_size(px(25.))
                .font_weight(FontWeight::BOLD)
                .child(page.label()),
        )
        .child(div().text_color(theme.muted).child(page.description()))
}

fn settings_page_content(page: SettingsPage, theme: Theme, body: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .max_w(px(1200.))
        .mx_auto()
        .p_6()
        .child(settings_page_header(page, theme))
        .child(body)
}

#[derive(Default)]
pub(super) struct ProfileSearch {
    pub searching: bool,
    pub message: Option<String>,
}
impl Global for ProfileSearch {}

pub(super) struct Preferences {
    pub(super) language: Entity<Select>,
    pub(super) profile: Entity<Select>,
    pub(super) behavior: Entity<Select>,
    pub(super) theme: Entity<Select>,
    pub(super) font: Entity<Select>,
    pub(super) size: Entity<NumberInput>,
    pub(super) line_height: Entity<NumberInput>,
    pub(super) cell_width: Entity<NumberInput>,
    pub(super) history: Entity<NumberInput>,
    _subscriptions: Vec<Subscription>,
}

pub(super) struct NumberInput {
    pub(super) input: Entity<TextField>,
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
}
impl NumberInput {
    fn new(
        label: &'static str,
        initial: impl ToString,
        min: f64,
        max: f64,
        step: f64,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = initial.to_string();
        let initial = text.parse().expect("numeric setting");
        Self {
            input: cx.new(|cx| TextField::new(text, label, cx)),
            label,
            min,
            max,
            step,
            initial,
        }
    }

    pub(super) fn value(&self, cx: &App) -> Option<f64> {
        let value = self.input.read(cx).text.trim().parse::<f64>().ok()?;
        (value.is_finite() && value >= self.min && value <= self.max).then_some(value)
    }

    fn step(&mut self, direction: f64, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.value(cx).unwrap_or(self.initial);
        let value = (value + direction * self.step).clamp(self.min, self.max);
        // Decimal increments such as 0.1 should not expose binary rounding noise.
        let value = (value * 1_000_000.).round() / 1_000_000.;
        self.input.update(cx, |input, cx| {
            input.set_text(value.to_string(), window, cx)
        });
        cx.notify();
    }
}
impl Render for NumberInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::current(cx);
        div()
            .id("number-input")
            .flex()
            .items_center()
            .gap_1()
            .w(px(150.))
            .h(px(36.))
            .aria_label(self.label)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.input.read(cx).is_composing() {
                    return;
                }
                let direction = match event.keystroke.key.as_str() {
                    "up" => 1.,
                    "down" => -1.,
                    _ => return,
                };
                this.step(direction, window, cx);
                cx.stop_propagation();
                window.prevent_default();
            }))
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        icon_button(
                            "increase",
                            Icon::ArrowUp,
                            crate::i18n::tr("increase_value"),
                            theme,
                        )
                        .w(px(26.))
                        .h(px(17.))
                        .on_click(cx.listener(|this, _, window, cx| this.step(1., window, cx))),
                    )
                    .child(
                        icon_button(
                            "decrease",
                            Icon::ArrowDown,
                            crate::i18n::tr("decrease_value"),
                            theme,
                        )
                        .w(px(26.))
                        .h(px(17.))
                        .on_click(cx.listener(|this, _, window, cx| this.step(-1., window, cx))),
                    ),
            )
    }
}

impl TerminalApp {
    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let nav = div()
            .w(px(168.))
            .h_full()
            .flex_shrink_0()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(16.))
                    .font_weight(FontWeight::BOLD)
                    .child(crate::i18n::tr("settings")),
            )
            .children(
                [
                    SettingsPage::Behavior,
                    SettingsPage::Appearance,
                    SettingsPage::ColorSchemes,
                    SettingsPage::Profiles,
                    SettingsPage::Shortcuts,
                ]
                .into_iter()
                .enumerate()
                .map(|(index, page)| {
                    button(("settings-page", index), page.label(), theme)
                        .when(
                            page == SettingsPage::Behavior && self.has_update(cx),
                            |button| {
                                button
                                    .aria_label(crate::i18n::tr("behavior_update_available"))
                                    .child(update_dot(theme).ml_2())
                            },
                        )
                        .justify_start()
                        .px_3()
                        .py_2()
                        .bg(if self.settings_page == page {
                            theme.hover
                        } else {
                            theme.bg
                        })
                        .aria_selected(self.settings_page == page)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings_page = page;
                            this.settings_scroll.set_offset(point(px(0.), px(0.)));
                            this.settings_scroll_drag = None;
                            this.shortcut_capture = None;
                            cx.notify();
                        }))
                }),
            );
        let (body, scroll_id, scroll) = match self.settings_page {
            SettingsPage::Behavior | SettingsPage::Appearance | SettingsPage::Profiles => (
                self.preferences_content(theme, cx),
                "settings-scroll",
                &self.settings_scroll,
            ),
            SettingsPage::ColorSchemes => (
                self.color_schemes_content(theme, cx),
                "settings-scroll",
                &self.settings_scroll,
            ),
            SettingsPage::Shortcuts => (
                self.shortcuts_content(theme, cx),
                "shortcuts-scroll",
                &self.shortcuts_scroll,
            ),
        };
        let content = self
            .settings_scroll_area(
                scroll_id,
                scroll,
                settings_page_content(self.settings_page, theme, body),
                theme,
                cx,
            )
            .min_w_0();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(div().flex().flex_1().min_h_0().child(nav).child(content))
            .child(self.settings_footer(theme, cx))
            .into_any_element()
    }

    /// Construct UI state only when a settings tab is actually rendered. In
    /// particular this does not request or enumerate installed font families.
    pub(super) fn ensure_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences.is_some() {
            return;
        }
        if !self.config_error
            && !self.dir.join("settings.toml").exists()
            && let Err(error) = self.settings.save(&self.dir)
        {
            self.status = Some(
                crate::tf!("failed_to_create_settings_file_error", error => format!("{:#}", error)),
            );
        }
        let language = cx.new(|cx| {
            let mut choices = vec![Choice::new(
                "system",
                crate::i18n::tr("follow_system_language"),
            )];
            choices.extend(
                crate::i18n::LOCALES
                    .iter()
                    .map(|locale| Choice::new(locale.code, locale.native_name)),
            );
            Select::new(
                crate::i18n::tr("language"),
                choices,
                if self.draft.language == "system" {
                    "system"
                } else {
                    crate::i18n::locale_for_tag(&self.draft.language).code
                }
                .into(),
                false,
                window,
                cx,
            )
        });
        let selected_profile = self
            .profiles
            .iter()
            .find(|p| p.id == self.draft.default_profile)
            .or_else(|| self.profiles.first())
            .map(|p| p.id.clone())
            .unwrap_or_default();
        let profile = cx.new(|cx| {
            Select::new(
                crate::i18n::tr("default_profile"),
                self.profiles
                    .iter()
                    .map(|p| {
                        Choice::new(p.id.clone(), p.name.clone())
                            .with_profile_icon(p.program.clone())
                    })
                    .collect(),
                selected_profile,
                false,
                window,
                cx,
            )
        });
        let behavior = cx.new(|cx| {
            Select::new(
                crate::i18n::tr("new_instance_behavior"),
                config::WindowingBehavior::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(i, behavior)| Choice::new(i.to_string(), behavior.label()))
                    .collect(),
                config::WindowingBehavior::ALL
                    .iter()
                    .position(|b| *b == self.draft.windowing_behavior)
                    .unwrap_or(0)
                    .to_string(),
                false,
                window,
                cx,
            )
        });
        let theme = cx.new(|cx| {
            Select::new(
                crate::i18n::tr("theme"),
                [
                    ("system", crate::i18n::tr("system")),
                    ("dark", crate::i18n::tr("dark")),
                    ("light", crate::i18n::tr("light")),
                ]
                .into_iter()
                .map(|(value, label)| Choice::new(value, label))
                .collect(),
                self.draft.theme.clone(),
                false,
                window,
                cx,
            )
        });
        let font = cx.new(|cx| {
            Select::new(
                crate::i18n::tr("terminal_font"),
                vec![Choice::new(
                    self.draft.font_family.clone(),
                    self.draft.font_family.clone(),
                )],
                self.draft.font_family.clone(),
                true,
                window,
                cx,
            )
        });
        let size = cx.new(|cx| {
            NumberInput::new(
                crate::i18n::tr("font_size"),
                self.draft.font_size,
                8.,
                48.,
                1.,
                cx,
            )
        });
        let line_height = cx.new(|cx| {
            NumberInput::new(
                crate::i18n::tr("line_height_multiplier"),
                self.draft.line_height,
                0.5,
                3.,
                0.1,
                cx,
            )
        });
        let cell_width = cx.new(|cx| {
            NumberInput::new(
                crate::i18n::tr("cell_width_multiplier"),
                self.draft.cell_width,
                0.3,
                2.,
                0.05,
                cx,
            )
        });
        let history = cx.new(|cx| {
            NumberInput::new(
                crate::i18n::tr("scrollback_lines"),
                self.draft.scrollback,
                0.,
                1_000_000.,
                1000.,
                cx,
            )
        });
        let mut subscriptions = vec![
            cx.subscribe(&language, |this, _, event: &Selected, cx| {
                this.draft.language = event.0.clone();
                cx.notify();
            }),
            cx.subscribe(&profile, |this, _, event: &Selected, cx| {
                this.draft.default_profile = event.0.clone();
                cx.notify();
            }),
            cx.subscribe(&behavior, |this, _, event: &Selected, cx| {
                if let Some(value) = event
                    .0
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| config::WindowingBehavior::ALL.get(i))
                {
                    this.draft.windowing_behavior = *value;
                }
                cx.notify();
            }),
            cx.subscribe(&theme, |this, _, event: &Selected, cx| {
                this.draft.theme = event.0.clone();
                cx.notify();
            }),
            cx.subscribe(&font, |this, _, event: &Selected, cx| {
                this.draft.font_family = event.0.clone();
                cx.notify();
            }),
        ];
        for number in [&size, &line_height, &cell_width, &history] {
            let input = number.read(cx).input.clone();
            subscriptions.push(cx.subscribe(&input, |_, _, event: &FieldEvent, cx| {
                if matches!(event, FieldEvent::Changed) {
                    cx.notify();
                }
            }));
        }
        self.preferences = Some(Preferences {
            language,
            profile,
            behavior,
            theme,
            font,
            size,
            line_height,
            cell_width,
            history,
            _subscriptions: subscriptions,
        });
    }

    pub(super) fn read_preferences(&mut self, cx: &App) -> Result<(), &'static str> {
        self.draft = self.edited_settings(cx)?;
        Ok(())
    }

    pub(super) fn has_settings_changes(&self, cx: &App) -> bool {
        !self
            .edited_settings(cx)
            .is_ok_and(|settings| settings == self.settings)
    }

    fn edited_settings(&self, cx: &App) -> Result<Settings, &'static str> {
        let mut settings = self.draft.clone();
        if let Some(prefs) = &self.preferences {
            let size = prefs
                .size
                .read(cx)
                .value(cx)
                .ok_or(crate::i18n::tr("enter_a_font_size_between_8_and_48"))?;
            let history = prefs
                .history
                .read(cx)
                .value(cx)
                .filter(|value| value.fract() == 0.)
                .ok_or(crate::i18n::tr(
                    "enter_a_scrollback_integer_between_0_and_1000000",
                ))?;
            let line_height = prefs.line_height.read(cx).value(cx).ok_or(crate::i18n::tr(
                "enter_a_line_height_multiplier_between_0_5_and_3_0",
            ))?;
            let cell_width = prefs.cell_width.read(cx).value(cx).ok_or(crate::i18n::tr(
                "enter_a_cell_width_multiplier_between_0_3_and_2_0",
            ))?;
            settings.font_size = size as f32;
            settings.line_height = line_height as f32;
            settings.cell_width = cell_width as f32;
            settings.scrollback = history as usize;
        }
        Ok(settings)
    }

    pub(super) fn search_profiles(&mut self, cx: &mut Context<Self>) {
        if cx.global::<ProfileSearch>().searching {
            return;
        }
        if self.config_error {
            self.status = Some(
                crate::i18n::tr("fix_the_settings_file_errors_before_searching_for_profiles")
                    .into(),
            );
            cx.notify();
            return;
        }
        cx.set_global(ProfileSearch {
            searching: true,
            message: Some(
                crate::i18n::tr("searching_for_installed_shells_and_wsl_distributions").into(),
            ),
        });
        let dir = self.dir.clone();
        cx.spawn(async move |_, cx| {
            let found = cx
                .background_executor()
                .spawn(async { platform::discover_profiles() })
                .await;
            cx.update(|cx| {
                let count = found.len();
                let mut settings = cx.global::<windows::SharedSettings>().0.clone();
                settings.discovered_profiles = found;
                let message = match settings.save(&dir) {
                    Ok(()) => {
                        cx.set_global(windows::SharedSettings(settings));
                        crate::tf!("found_and_saved_count_profiles", count => count)
                    }
                    Err(error) => crate::tf!("failed_to_save_search_results_error", error => format!("{:#}", error)),
                };
                cx.set_global(ProfileSearch {
                    searching: false,
                    message: Some(message),
                });
            });
        })
        .detach();
    }

    fn preferences_content(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let prefs = self
            .preferences
            .as_ref()
            .expect("settings UI initialized on render");
        let selector = |entity: &Entity<Select>| div().w_full().child(entity.clone());
        let number = |entity: &Entity<NumberInput>| {
            div().w_full().flex().justify_end().child(entity.clone())
        };
        let content = div().flex().flex_col().gap_2();
        match self.settings_page {
            SettingsPage::Behavior => content
                .child(self.update_view(theme, cx))
                .child(setting(
                    crate::i18n::tr("language"),
                    crate::i18n::tr("language_description"),
                    theme,
                    self.draft.language != self.settings.language,
                    selector(&prefs.language),
                ))
                .child(setting(
                    crate::i18n::tr("default_profile"),
                    crate::i18n::tr("the_shell_launched_by_the_button_for_new_tabs"),
                    theme,
                    self.draft.default_profile != self.settings.default_profile,
                    selector(&prefs.profile),
                ))
                .when(cfg!(windows), |content| {
                    #[cfg(windows)]
                    {
                        content.child(setting(
                            crate::i18n::tr("windows_default_terminal"),
                            crate::i18n::tr("open_newly_launched_consoles_in_edolterm"),
                            theme,
                            false,
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    settings_button(
                                        "set-default-terminal",
                                        crate::i18n::tr("set_as_default_terminal"),
                                        theme,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| this.change_default_terminal(false, cx),
                                    )),
                                )
                                .child(
                                    settings_button(
                                        "restore-default-terminal",
                                        crate::i18n::tr("restore_previous_setting"),
                                        theme,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| this.change_default_terminal(true, cx),
                                    )),
                                ),
                        ))
                    }
                    #[cfg(not(windows))]
                    {
                        content
                    }
                })
                .child(setting(
                    crate::i18n::tr("new_instance_behavior"),
                    crate::i18n::tr(
                        "the_window_used_when_launching_from_explorer_or_a_shortcut_creates_a_w",
                    ),
                    theme,
                    self.draft.windowing_behavior != self.settings.windowing_behavior,
                    selector(&prefs.behavior),
                ))
                .child(setting(
                    crate::i18n::tr("scrollback"),
                    crate::i18n::tr("the_number_of_lines_kept_per_tab_up_to_1_000_000"),
                    theme,
                    prefs.history.read(cx).value(cx) != Some(self.settings.scrollback as f64),
                    number(&prefs.history),
                ))
                .child(setting(
                    crate::i18n::tr("zoom_with_ctrl_scroll"),
                    crate::i18n::tr(
                        "hold_ctrl_and_scroll_up_to_increase_the_current_tab_s_font_size_or_dow",
                    ),
                    theme,
                    self.draft.ctrl_scroll_zoom != self.settings.ctrl_scroll_zoom,
                    div().flex().justify_end().child(
                        toggle(
                            "ctrl-scroll-zoom",
                            crate::i18n::tr("zoom_with_ctrl_scroll"),
                            self.draft.ctrl_scroll_zoom,
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.draft.ctrl_scroll_zoom = !this.draft.ctrl_scroll_zoom;
                            cx.notify();
                        })),
                    ),
                ))
                .child(setting(
                    crate::i18n::tr("detect_urls_automatically"),
                    crate::i18n::tr(
                        "detect_plain_urls_and_osc_8_links_hold_ctrl_to_underline_them_ctrl_cli",
                    ),
                    theme,
                    self.draft.detect_urls != self.settings.detect_urls,
                    div().flex().justify_end().child(
                        toggle(
                            "detect-urls",
                            crate::i18n::tr("detect_urls_automatically"),
                            self.draft.detect_urls,
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.draft.detect_urls = !this.draft.detect_urls;
                            cx.notify();
                        })),
                    ),
                ))
                .child(setting(
                    crate::i18n::tr("confirm_when_closing_a_window"),
                    crate::i18n::tr("ask_before_closing_a_window_with_open_sessions"),
                    theme,
                    self.draft.confirm_close != self.settings.confirm_close,
                    div().flex().justify_end().child(
                        toggle(
                            "confirm-close",
                            crate::i18n::tr("confirm_when_closing_a_window"),
                            self.draft.confirm_close,
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.draft.confirm_close = !this.draft.confirm_close;
                            cx.notify();
                        })),
                    ),
                ))
                .child(setting(
                    crate::i18n::tr("restore_session"),
                    crate::i18n::tr(
                        "restore_tabs_and_starting_directories_on_the_next_launch_running_comma",
                    ),
                    theme,
                    self.draft.restore_session != self.settings.restore_session,
                    div().flex().justify_end().child(
                        toggle(
                            "restore-session",
                            crate::i18n::tr("restore_session"),
                            self.draft.restore_session,
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.draft.restore_session = !this.draft.restore_session;
                            cx.notify();
                        })),
                    ),
                )),
            SettingsPage::Appearance => content
                .child(setting(
                    crate::i18n::tr("theme"),
                    crate::i18n::tr("system_follows_the_operating_system_theme"),
                    theme,
                    self.draft.theme != self.settings.theme,
                    selector(&prefs.theme),
                ))
                .child(setting(
                    crate::i18n::tr("terminal_font"),
                    crate::i18n::tr(
                        "search_and_select_an_installed_font_monospaced_fonts_are_recommended_f",
                    ),
                    theme,
                    self.draft.font_family != self.settings.font_family,
                    selector(&prefs.font),
                ))
                .child(setting(
                    crate::i18n::tr("font_size"),
                    crate::i18n::tr("terminal_font_size_from_8_to_48_px"),
                    theme,
                    prefs.size.read(cx).value(cx).map(|value| value as f32)
                        != Some(self.settings.font_size),
                    number(&prefs.size),
                ))
                .child(setting(
                    crate::i18n::tr("line_height"),
                    crate::i18n::tr("multiplied_by_the_font_size_default_1_2_range_0_5_3_0"),
                    theme,
                    prefs
                        .line_height
                        .read(cx)
                        .value(cx)
                        .map(|value| value as f32)
                        != Some(self.settings.line_height),
                    number(&prefs.line_height),
                ))
                .child(setting(
                    crate::i18n::tr("cell_width"),
                    crate::i18n::tr(
                        "horizontal_width_multiplied_by_the_font_size_default_0_6_range_0_3_2_0",
                    ),
                    theme,
                    prefs
                        .cell_width
                        .read(cx)
                        .value(cx)
                        .map(|value| value as f32)
                        != Some(self.settings.cell_width),
                    number(&prefs.cell_width),
                )),
            SettingsPage::Profiles => content
                .child(setting(
                    crate::i18n::tr("discover_profiles"),
                    crate::i18n::tr("find_installed_shells_and_update_the_saved_profile_list"),
                    theme,
                    false,
                    settings_button(
                        "discover-profiles",
                        if cx.global::<ProfileSearch>().searching {
                            crate::i18n::tr("searching")
                        } else {
                            crate::i18n::tr("search_profiles")
                        },
                        theme,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.search_profiles(cx))),
                ))
                .when_some(
                    cx.global::<ProfileSearch>().message.clone(),
                    |content, message| {
                        content.child(
                            div()
                                .px_5()
                                .text_size(px(12.))
                                .text_color(theme.muted)
                                .child(message),
                        )
                    },
                )
                .child(
                    div()
                        .mt_5()
                        .mb_2()
                        .text_size(px(16.))
                        .font_weight(FontWeight::BOLD)
                        .child(crate::i18n::tr("custom_profiles")),
                )
                .child(div().text_color(theme.description).child(crate::i18n::tr(
                    "use_open_settings_file_below_to_add_programs_arguments_starting_direct",
                )))
                .child(
                    div()
                        .mt_2()
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(self.dir.join("settings.toml").display().to_string()),
                ),
            SettingsPage::Shortcuts | SettingsPage::ColorSchemes => unreachable!(),
        }
    }

    fn color_schemes_content(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .tab_group()
            .flex()
            .flex_col()
            .gap_2()
            .children(COLOR_SCHEMES.iter().enumerate().map(|(index, scheme)| {
                let selected = self.draft.color_scheme == scheme.name;
                scheme_preview(scheme)
                    .id(("color-scheme", index))
                    .role(Role::Button)
                    .aria_label(scheme.name)
                    .aria_selected(selected)
                    .tab_index(0)
                    .cursor_pointer()
                    .p_5()
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .hover(move |style| style.border_color(theme.accent))
                    .focus(move |style| style.border_color(theme.accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_color_scheme(scheme.name, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "tab" {
                            if event.keystroke.modifiers.shift {
                                window.focus_prev(cx);
                            } else {
                                window.focus_next(cx);
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.choose_color_scheme(scheme.name, cx);
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    }))
            }))
    }

    pub(super) fn choose_color_scheme(&mut self, name: &str, cx: &mut Context<Self>) {
        self.draft.color_scheme = name.into();
        cx.notify();
    }

    pub(super) fn open_settings_file(&mut self, cx: &mut Context<Self>) {
        let path = self.dir.join("settings.toml");
        let result = if path.exists() {
            Ok(())
        } else {
            self.settings.save(&self.dir)
        }
        .and_then(|_| platform::open_file(&path));
        if let Err(error) = result {
            self.status = Some(error.to_string());
        }
        cx.notify();
    }

    fn settings_footer(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("settings-footer")
            .flex()
            .flex_wrap()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .gap_3()
            .w_full()
            .px_6()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        settings_button("reload-config", crate::i18n::tr("reload_settings"), theme)
                            .on_click(cx.listener(|this, _, _, cx| this.reload_settings(cx))),
                    )
                    .child(
                        settings_button(
                            "edit-config",
                            crate::i18n::tr("open_settings_file"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.open_settings_file(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .ml_auto()
                    .child(
                        settings_button(
                            "reset-settings",
                            crate::i18n::tr("reset_all_to_defaults"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.reset_settings(cx))),
                    )
                    .child(
                        settings_button(
                            "cancel-settings",
                            crate::i18n::tr("discard_changes"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_settings_changes(cx))),
                    )
                    .child(
                        settings_action(
                            "save-settings",
                            crate::i18n::tr("save_settings"),
                            theme,
                            self.has_settings_changes(cx),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
    }
}

#[cfg(windows)]
impl TerminalApp {
    fn change_default_terminal(&mut self, restore: bool, cx: &mut Context<Self>) {
        self.status = Some(crate::i18n::tr("changing_the_default_terminal_setting").into());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::handoff::change_default(restore) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status = Some(match result {
                    Ok(message) if !message.is_empty() => message,
                    Ok(_) => {
                        crate::i18n::tr("the_default_terminal_setting_has_been_checked").into()
                    }
                    Err(error) => format!("{error:#}"),
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn toggle(id: &'static str, label: &'static str, enabled: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(label)
        .aria_toggled(if enabled {
            Toggled::True
        } else {
            Toggled::False
        })
        .flex()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .child(div().text_color(theme.muted).child(if enabled {
            crate::i18n::tr("on")
        } else {
            crate::i18n::tr("off")
        }))
        .child(
            div()
                .flex()
                .items_center()
                .w(px(44.))
                .h(px(24.))
                .px(px(3.))
                .rounded_full()
                .border_1()
                .border_color(if enabled { theme.accent } else { theme.muted })
                .bg(if enabled { theme.accent } else { theme.bg })
                .when(enabled, |track| track.justify_end())
                .child(div().size(px(16.)).rounded_full().bg(if enabled {
                    theme.bg
                } else {
                    theme.muted
                })),
        )
}

fn setting(
    label: &'static str,
    description: &'static str,
    theme: Theme,
    changed: bool,
    control: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_5()
        .w_full()
        .min_h(px(76.))
        .px_5()
        .py_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(if changed {
            theme.unsaved.bg
        } else {
            theme.panel
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(15.))
                        .when(changed, |label| label.text_color(theme.unsaved.fg))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme.description)
                        .child(description),
                ),
        )
        .child(
            div()
                .w(relative(0.43))
                .max_w(px(360.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_end()
                .child(control),
        )
}

/// Use the actual scheme colors so light and dark palettes can be compared in any UI theme.
fn scheme_preview(scheme: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_3()
        .p_3()
        .rounded_md()
        .bg(rgb(scheme.background))
        .text_color(rgb(scheme.foreground))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .flex_shrink_0()
                .children(scheme.ansi.chunks(8).map(|row| {
                    div().flex().gap(px(3.)).children(
                        row.iter()
                            .map(|color| div().size(px(14.)).rounded_sm().bg(rgb(*color))),
                    )
                })),
        )
        .child(div().flex_1().min_w(px(130.)).child(scheme.name))
        .child(div().text_size(px(12.)).child("Aa 가나다 0123"))
}
