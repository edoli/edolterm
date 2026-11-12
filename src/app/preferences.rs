use super::*;
use crate::select::{Choice, Select, Selected};

#[derive(Default)]
pub(super) struct ProfileSearch {
    pub searching: bool,
    pub message: Option<String>,
}
impl Global for ProfileSearch {}

pub(super) struct Preferences {
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
                        icon_button("increase", Icon::ArrowUp, "값 늘리기", theme)
                            .w(px(26.))
                            .h(px(17.))
                            .on_click(cx.listener(|this, _, window, cx| this.step(1., window, cx))),
                    )
                    .child(
                        icon_button("decrease", Icon::ArrowDown, "값 줄이기", theme)
                            .w(px(26.))
                            .h(px(17.))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.step(-1., window, cx)),
                            ),
                    ),
            )
    }
}

impl TerminalApp {
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
            self.status = Some(format!("설정 파일을 만들지 못했습니다: {error:#}"));
        }
        let selected_profile = self
            .profiles
            .iter()
            .find(|p| p.id == self.draft.default_profile)
            .or_else(|| self.profiles.first())
            .map(|p| p.id.clone())
            .unwrap_or_default();
        let profile = cx.new(|cx| {
            Select::new(
                "기본 프로필",
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
                "새 인스턴스 동작",
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
                "테마",
                [("system", "시스템"), ("dark", "다크"), ("light", "라이트")]
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
                "터미널 글꼴",
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
        let size =
            cx.new(|cx| NumberInput::new("글꼴 크기", self.draft.font_size, 8., 48., 1., cx));
        let line_height =
            cx.new(|cx| NumberInput::new("줄 높이 배율", self.draft.line_height, 0.5, 3., 0.1, cx));
        let cell_width =
            cx.new(|cx| NumberInput::new("셀 너비 배율", self.draft.cell_width, 0.3, 2., 0.05, cx));
        let history = cx.new(|cx| {
            NumberInput::new(
                "스크롤백 줄 수",
                self.draft.scrollback,
                0.,
                1_000_000.,
                1000.,
                cx,
            )
        });
        let mut subscriptions = vec![
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
                .ok_or("글꼴 크기에 8–48 사이의 숫자를 입력하세요.")?;
            let history = prefs
                .history
                .read(cx)
                .value(cx)
                .filter(|value| value.fract() == 0.)
                .ok_or("스크롤백에 0–1000000 사이의 정수를 입력하세요.")?;
            let line_height = prefs
                .line_height
                .read(cx)
                .value(cx)
                .ok_or("줄 높이 배율에 0.5–3.0 사이의 숫자를 입력하세요.")?;
            let cell_width = prefs
                .cell_width
                .read(cx)
                .value(cx)
                .ok_or("셀 너비 배율에 0.3–2.0 사이의 숫자를 입력하세요.")?;
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
            self.status = Some("설정 파일 오류를 해결한 뒤 프로필을 검색해 주세요.".into());
            cx.notify();
            return;
        }
        cx.set_global(ProfileSearch {
            searching: true,
            message: Some("설치된 셸과 WSL 배포판을 검색하고 있습니다…".into()),
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
                        format!("{count}개 프로필을 찾아 저장했습니다.")
                    }
                    Err(error) => format!("검색 결과를 저장하지 못했습니다: {error:#}"),
                };
                cx.set_global(ProfileSearch {
                    searching: false,
                    message: Some(message),
                });
            });
        })
        .detach();
    }

    pub(super) fn preferences_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let prefs = self
            .preferences
            .as_ref()
            .expect("settings UI initialized on render");
        let selector = |entity: &Entity<Select>| div().w_full().child(entity.clone());
        let number = |entity: &Entity<NumberInput>| {
            div().w_full().flex().justify_end().child(entity.clone())
        };
        let title = match self.settings_page {
            SettingsPage::Behavior => "동작",
            SettingsPage::Appearance => "모양",
            SettingsPage::Profiles => "프로필",
            SettingsPage::Shortcuts => unreachable!(),
        };
        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .max_w(px(1200.))
            .mx_auto()
            .p_6()
            .child(
                div()
                    .mb_1()
                    .text_size(px(25.))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .mb_4()
                    .text_color(theme.muted)
                    .child("설정은 저장하면 열린 터미널에도 적용됩니다."),
            );
        let content = match self.settings_page {
            SettingsPage::Behavior => content
                .child(self.update_view(theme, cx))
                .child(setting("기본 프로필", "새 탭의 + 버튼으로 실행할 셸입니다.", theme, self.draft.default_profile != self.settings.default_profile, selector(&prefs.profile)))
                .when(cfg!(windows), |content| {
                    #[cfg(windows)]
                    { content.child(setting("Windows 기본 터미널", "새로 실행하는 콘솔을 Edolterm으로 엽니다.", theme, false,
                        div().flex().flex_wrap().gap_2()
                            .child(settings_button("set-default-terminal", "기본 터미널로 설정", theme).on_click(cx.listener(|this, _, _, cx| this.change_default_terminal(false, cx))))
                            .child(settings_button("restore-default-terminal", "이전 설정 복원", theme).on_click(cx.listener(|this, _, _, cx| this.change_default_terminal(true, cx)))))) }
                    #[cfg(not(windows))]
                    { content }
                })
                .child(setting("새 인스턴스 동작", "탐색기나 바로 가기에서 실행할 때 사용할 창입니다. 대상 창이 없으면 새 창을 만듭니다.", theme, self.draft.windowing_behavior != self.settings.windowing_behavior, selector(&prefs.behavior)))
                .child(setting("스크롤백", "탭마다 보관할 줄 수입니다. 최대 1,000,000줄까지 설정할 수 있습니다.", theme, prefs.history.read(cx).value(cx) != Some(self.settings.scrollback as f64), number(&prefs.history)))
                .child(setting("창을 닫을 때 확인", "열린 세션이 있으면 창을 닫기 전에 확인합니다.", theme, self.draft.confirm_close != self.settings.confirm_close,
                    div().flex().justify_end().child(toggle("confirm-close", "창을 닫을 때 확인", self.draft.confirm_close, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.confirm_close = !this.draft.confirm_close; cx.notify(); })))))
                .child(setting("세션 복원", "다음 실행 시 탭과 시작 폴더를 복원합니다. 실행 중인 명령과 출력은 복원되지 않습니다.", theme, self.draft.restore_session != self.settings.restore_session,
                    div().flex().justify_end().child(toggle("restore-session", "세션 복원", self.draft.restore_session, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.restore_session = !this.draft.restore_session; cx.notify(); }))))),
            SettingsPage::Appearance => content
                .child(setting("테마", "시스템을 선택하면 운영체제의 테마 설정을 따릅니다.", theme, self.draft.theme != self.settings.theme, selector(&prefs.theme)))
                .child(setting("터미널 글꼴", "설치된 글꼴을 검색해 선택합니다. 터미널에는 고정폭 글꼴을 권장합니다.", theme, self.draft.font_family != self.settings.font_family, selector(&prefs.font)))
                .child(setting("글꼴 크기", "터미널의 글꼴 크기입니다. 8–48 px 사이로 설정합니다.", theme, prefs.size.read(cx).value(cx).map(|value| value as f32) != Some(self.settings.font_size), number(&prefs.size)))
                .child(setting("줄 높이", "글꼴 크기에 곱할 배율입니다. 기본값 1.2 · 범위 0.5–3.0", theme, prefs.line_height.read(cx).value(cx).map(|value| value as f32) != Some(self.settings.line_height), number(&prefs.line_height)))
                .child(setting("셀 너비", "글꼴 크기에 곱할 가로 폭의 배율입니다. 기본값 0.6 · 범위 0.3–2.0", theme, prefs.cell_width.read(cx).value(cx).map(|value| value as f32) != Some(self.settings.cell_width), number(&prefs.cell_width))),
            SettingsPage::Profiles => content
                .child(setting("프로필 자동 검색", "시스템에 있는 쉘을 자동으로 검색해 저장된 목록을 갱신합니다.", theme, false,
                    settings_button("discover-profiles", if cx.global::<ProfileSearch>().searching { "검색 중…" } else { "프로필 검색" }, theme)
                        .on_click(cx.listener(|this, _, _, cx| this.search_profiles(cx)))))
                .when_some(cx.global::<ProfileSearch>().message.clone(), |content, message| content.child(div().px_5().text_size(px(12.)).text_color(theme.muted).child(message)))
                .child(div().mt_5().mb_2().text_size(px(16.)).font_weight(FontWeight::BOLD).child("사용자 지정 프로필"))
                .child(div().text_color(theme.description).child("하단의 ‘설정 파일 열기’로 파일을 열고 [[profiles]]에 실행 파일, 인자, 시작 폴더, 환경변수를 추가할 수 있습니다. 편집한 뒤 ‘설정 다시 불러오기’를 누르세요."))
                .child(div().mt_2().text_size(px(12.)).text_color(theme.muted).child(self.dir.join("settings.toml").display().to_string())),
            SettingsPage::Shortcuts => unreachable!(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(self.settings_scroll_area(
                "settings-scroll",
                &self.settings_scroll,
                content,
                theme,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn settings_footer(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
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
                        settings_button("reload-config", "설정 다시 불러오기", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.reload_settings(cx))),
                    )
                    .child(
                        settings_button("edit-config", "설정 파일 열기", theme).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.run_shortcut(keybindings::Action::SettingsFile, window, cx);
                            }),
                        ),
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
                        settings_button("reset-settings", "모두 기본값으로", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.reset_settings(cx))),
                    )
                    .child(
                        settings_button("cancel-settings", "변경 내용 취소", theme).on_click(
                            cx.listener(|this, _, _, cx| this.cancel_settings_changes(cx)),
                        ),
                    )
                    .child(
                        settings_action(
                            "save-settings",
                            "설정 저장",
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
        self.status = Some("기본 터미널 설정을 변경하고 있습니다…".into());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::handoff::change_default(restore) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status = Some(match result {
                    Ok(message) if !message.is_empty() => message,
                    Ok(_) => "기본 터미널 설정을 확인했습니다.".into(),
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
        .child(
            div()
                .text_color(theme.muted)
                .child(if enabled { "켬" } else { "끔" }),
        )
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
