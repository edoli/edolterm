//! One release check per process, deferred until startup shells are ready.
use super::*;

const QUIET_PERIOD: Duration = Duration::from_secs(2);
const UNINTEGRATED_GRACE: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Checks {
    pub state: crate::update::State,
    pub(super) started: bool,
    #[cfg(debug_assertions)]
    pub(super) preview: bool,
}
impl Global for Checks {}

#[derive(Clone, Copy)]
enum ShellReadiness {
    Starting,
    AwaitingPrompt,
    WithoutIntegration,
    Ready,
}

#[derive(Default)]
struct StartupGate {
    unintegrated_since: Option<Instant>,
    quiet_since: Option<Instant>,
}

impl StartupGate {
    fn ready(&mut self, shells: &[ShellReadiness], now: Instant) -> bool {
        if shells.iter().any(|state| {
            matches!(
                state,
                ShellReadiness::Starting | ShellReadiness::AwaitingPrompt
            )
        }) {
            self.unintegrated_since = None;
            self.quiet_since = None;
            return false;
        }
        if shells
            .iter()
            .any(|state| matches!(state, ShellReadiness::WithoutIntegration))
            && now.duration_since(*self.unintegrated_since.get_or_insert(now)) < UNINTEGRATED_GRACE
        {
            self.quiet_since = None;
            return false;
        }
        now.duration_since(*self.quiet_since.get_or_insert(now)) >= QUIET_PERIOD
    }
}

fn expects_prompt(profile: &Profile) -> bool {
    let name = profile
        .program
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match name.as_str() {
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => profile.args.iter().all(|arg| {
            matches!(
                arg.to_ascii_lowercase().as_str(),
                "-nologo" | "-noprofile" | "-noexit" | "-interactive" | "-nol" | "-nop" | "-noe"
            )
        }),
        "cmd" | "cmd.exe" => !profile
            .args
            .iter()
            .any(|arg| arg.to_ascii_lowercase().starts_with("/c")),
        _ => false,
    }
}

pub(super) fn schedule(root: WeakEntity<TerminalApp>, window: &Window) {
    if cfg!(windows) {
        schedule_with(root, window, check);
    }
}

pub(super) fn schedule_with(
    root: WeakEntity<TerminalApp>,
    window: &Window,
    check: impl FnOnce(&mut App) -> bool + 'static,
) {
    taskbar::after_startup(root, window, move |cx| {
        cx.spawn(async move |cx| {
            let mut gate = StartupGate::default();
            loop {
                let shells = cx.update(|cx| {
                    if cx.global::<Checks>().started {
                        return None;
                    }
                    let mut shells = Vec::new();
                    let mut live_window = false;
                    for entry in &cx.global::<windows::Windows>().entries {
                        let Some(root) = entry.root.upgrade() else {
                            continue;
                        };
                        let app = root.read(cx);
                        if app.closing {
                            continue;
                        }
                        live_window = true;
                        for terminal in app.tabs.iter().filter_map(|tab| tab.terminal.as_ref()) {
                            let terminal = terminal.read(cx);
                            shells.push(if terminal.error.is_some() || terminal.exited {
                                ShellReadiness::Ready
                            } else if let Some(session) = &terminal.session {
                                if session.prompt_reported() {
                                    ShellReadiness::Ready
                                } else if expects_prompt(&terminal.profile) {
                                    ShellReadiness::AwaitingPrompt
                                } else {
                                    ShellReadiness::WithoutIntegration
                                }
                            } else {
                                ShellReadiness::Starting
                            });
                        }
                    }
                    live_window.then_some(shells)
                });
                let Some(shells) = shells else { return };
                if gate.ready(&shells, Instant::now()) {
                    cx.update(check);
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
            }
        })
        .detach();
    });
}

pub(super) fn check(cx: &mut App) -> bool {
    check_with(cx, crate::update::check)
}

pub(super) fn check_with(
    cx: &mut App,
    work: impl FnOnce() -> anyhow::Result<crate::update::State> + Send + 'static,
) -> bool {
    #[cfg(debug_assertions)]
    if cx.global::<Checks>().preview {
        return false;
    }
    if cx.global::<Checks>().state.is_busy() {
        return false;
    }
    cx.set_global(Checks {
        state: crate::update::State::Checking,
        started: true,
        #[cfg(debug_assertions)]
        preview: false,
    });
    platform::trace_startup("update_check_start");
    let task = cx.background_executor().spawn(async move { work() });
    cx.spawn(async move |cx| {
        let state = task
            .await
            .unwrap_or_else(|error| crate::update::State::Failed(error.to_string()));
        cx.update(|cx| {
            cx.set_global(Checks {
                state,
                started: true,
                #[cfg(debug_assertions)]
                preview: false,
            })
        });
    })
    .detach();
    true
}

#[cfg(test)]
mod tests {
    use super::{ShellReadiness, StartupGate, expects_prompt};
    use crate::config::Profile;
    use std::time::{Duration, Instant};

    #[test]
    fn slow_integrated_prompts_and_restored_tabs_block_checks() {
        let start = Instant::now();
        let mut gate = StartupGate::default();
        assert!(!gate.ready(
            &[ShellReadiness::AwaitingPrompt],
            start + Duration::from_secs(30)
        ));
        assert!(!gate.ready(
            &[ShellReadiness::Ready, ShellReadiness::Starting],
            start + Duration::from_secs(40)
        ));
        assert!(!gate.ready(&[ShellReadiness::Ready], start + Duration::from_secs(41)));
        assert!(gate.ready(&[ShellReadiness::Ready], start + Duration::from_secs(43)));
        assert!(!gate.ready(&[ShellReadiness::Starting], start + Duration::from_secs(44)));
        assert!(!gate.ready(&[ShellReadiness::Ready], start + Duration::from_secs(45)));
    }

    #[test]
    fn unintegrated_shells_get_a_grace_period_after_attachment() {
        let start = Instant::now();
        let mut gate = StartupGate::default();
        assert!(!gate.ready(&[ShellReadiness::Starting], start));
        assert!(!gate.ready(
            &[ShellReadiness::WithoutIntegration],
            start + Duration::from_secs(20)
        ));
        assert!(!gate.ready(
            &[ShellReadiness::WithoutIntegration],
            start + Duration::from_secs(29)
        ));
        assert!(!gate.ready(
            &[ShellReadiness::WithoutIntegration],
            start + Duration::from_secs(30)
        ));
        assert!(gate.ready(
            &[ShellReadiness::WithoutIntegration],
            start + Duration::from_secs(32)
        ));
    }

    #[test]
    fn wait_only_for_prompts_installed_by_shell_integration() {
        assert!(expects_prompt(&Profile::new(
            "ps",
            "PowerShell",
            "C:/Program Files/PowerShell/pwsh.exe",
            &["-NoLogo"]
        )));
        assert!(expects_prompt(&Profile::new("cmd", "CMD", "cmd.exe", &[])));
        assert!(!expects_prompt(&Profile::new(
            "ps",
            "Command",
            "pwsh.exe",
            &["-Command", "test"]
        )));
        assert!(!expects_prompt(&Profile::new(
            "cmd",
            "Command",
            "cmd.exe",
            &["/c", "test"]
        )));
        assert!(!expects_prompt(&Profile::new("wsl", "WSL", "wsl.exe", &[])));
    }
}
