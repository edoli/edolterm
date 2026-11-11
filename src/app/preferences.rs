use super::*;
use crate::select::{Choice, Select, Selected};

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
        let subscriptions = vec![
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
            self.draft.font_size = size as f32;
            self.draft.line_height = line_height as f32;
            self.draft.cell_width = cell_width as f32;
            self.draft.scrollback = history as usize;
        }
        Ok(())
    }

    pub(super) fn general_settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::from_setting(&self.settings.theme, cx);
        let prefs = self
            .preferences
            .as_ref()
            .expect("settings UI initialized on render");
        let selector = |entity: &Entity<Select>| div().w_full().child(entity.clone());
        let number = |entity: &Entity<NumberInput>| {
            div().w_full().flex().justify_end().child(entity.clone())
        };
        let content = div().flex().flex_col().gap_2().w_full().max_w(px(1200.)).mx_auto().p_6()
            .child(self.update_view(theme, cx))
            .child(div().mb_1().text_size(px(25.)).font_weight(FontWeight::BOLD).child("설정"))
            .child(div().mb_4().text_color(theme.muted).child("설정은 저장하면 열린 터미널에도 적용됩니다."))
            .when(cfg!(windows), |content| {
                #[cfg(windows)]
                { content.child(setting("Windows 기본 터미널", "새로 실행하는 콘솔을 Edolterm으로 엽니다. Windows 11 또는 업데이트된 Windows 10 22H2와 Windows 연동 설치가 필요합니다.", theme,
                    div().flex().flex_wrap().gap_2()
                        .child(settings_button("set-default-terminal", "기본 터미널로 설정", theme).on_click(cx.listener(|this, _, _, cx| this.change_default_terminal(false, cx))))
                        .child(settings_button("restore-default-terminal", "이전 설정 복원", theme).on_click(cx.listener(|this, _, _, cx| this.change_default_terminal(true, cx)))))) }
                #[cfg(not(windows))]
                { content }
            })
            .child(setting("기본 프로필", "새 탭의 + 버튼으로 실행할 셸입니다.", theme, selector(&prefs.profile)))
            .child(setting("새 인스턴스 동작", "탐색기나 바로 가기에서 실행할 때 사용할 창입니다. 대상 창이 없으면 새 창을 만듭니다.", theme, selector(&prefs.behavior)))
            .child(setting("터미널 글꼴", "설치된 글꼴을 검색해 선택합니다. 터미널에는 고정폭 글꼴을 권장합니다.", theme, selector(&prefs.font)))
            .child(setting("글꼴 크기", "터미널의 글꼴 크기입니다. 8–48 px 사이로 설정합니다.", theme, number(&prefs.size)))
            .child(setting("줄 높이", "글꼴 크기에 곱할 배율입니다. 기본값 1.2 · 범위 0.5–3.0", theme, number(&prefs.line_height)))
            .child(setting("셀 너비", "글꼴 크기에 곱할 가로 폭의 배율입니다. 기본값 0.6 · 범위 0.3–2.0", theme, number(&prefs.cell_width)))
            .child(setting("스크롤백", "탭마다 보관할 줄 수입니다. 최대 1,000,000줄까지 설정할 수 있습니다.", theme, number(&prefs.history)))
            .child(setting("테마", "시스템을 선택하면 운영체제의 테마 설정을 따릅니다.", theme, selector(&prefs.theme)))
            .child(setting("창을 닫을 때 확인", "열린 세션이 있으면 창을 닫기 전에 확인합니다.", theme,
                div().flex().justify_end().child(toggle("confirm-close", "창을 닫을 때 확인", self.draft.confirm_close, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.confirm_close = !this.draft.confirm_close; cx.notify(); })))))
            .child(setting("세션 복원", "다음 실행 시 탭과 시작 폴더를 복원합니다. 실행 중인 명령과 출력은 복원되지 않습니다.", theme,
                div().flex().justify_end().child(toggle("restore-session", "세션 복원", self.draft.restore_session, theme).on_click(cx.listener(|this, _, _, cx| { this.draft.restore_session = !this.draft.restore_session; cx.notify(); })))))
            .child(div().mt_5().mb_2().text_size(px(16.)).font_weight(FontWeight::BOLD).child("사용자 지정 프로필"))
            .child(setting("프로필 편집", "설정 파일의 [[profiles]]에서 실행 파일, 인자, 시작 폴더, 환경변수를 추가할 수 있습니다.", theme,
                settings_button("edit-config", "설정 파일 열기", theme).on_click(cx.listener(|this, _, _, cx| {
                    if let Err(e) = platform::open_file(&this.dir.join("settings.toml")) { this.status = Some(format!("{e:#}")); cx.notify(); }
                }))))
            .child(setting("설정 다시 불러오기", "설정 파일을 직접 편집한 뒤 변경 내용을 불러옵니다.", theme,
                settings_button("reload-config", "다시 불러오기", theme).on_click(cx.listener(|this, _, _, cx| this.reload_settings(cx)))))
            .child(div().mt_2().text_size(px(12.)).text_color(theme.muted).child(self.dir.join("settings.toml").display().to_string()));
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
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_6()
                    .py_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        settings_button("reset-settings", "모두 기본값으로", theme).on_click(
                            cx.listener(|this, _, _, cx| this.reset_general_settings(cx)),
                        ),
                    )
                    .child(
                        primary_settings_button("save-settings", "설정 저장", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
                    ),
            )
            .into_any_element()
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
        .bg(theme.panel)
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
