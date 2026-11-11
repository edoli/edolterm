//! Launch requests and window selection are independent of GPUI and the PTY.
use crate::config::WindowingBehavior;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{Host, Outcome, start};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub id: String,
    pub directory: Option<PathBuf>,
    #[serde(default)]
    pub profile: Option<String>,
    pub new_window: bool,
    #[cfg(windows)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<crate::handoff::Descriptor>,
}

impl Request {
    pub fn new(directory: Option<PathBuf>, new_window: bool) -> anyhow::Result<Self> {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|e| anyhow::anyhow!("실행 요청 생성 실패: {e}"))?;
        Ok(Self {
            version: 1,
            id: nonce.iter().map(|byte| format!("{byte:02x}")).collect(),
            directory,
            profile: None,
            new_window,
            #[cfg(windows)]
            handoff: None,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(handoff) = &self.handoff {
            handoff.validate().map_err(|error| error.to_string())?;
            if self.profile.is_some() || self.directory.is_some() {
                return Err("전달된 콘솔에는 프로필이나 시작 폴더를 지정할 수 없습니다.".into());
            }
        }
        #[cfg(windows)]
        let valid_version = if self.handoff.is_some() {
            self.version == 2
        } else {
            self.version == 1
        };
        #[cfg(not(windows))]
        let valid_version = self.version == 1;
        if !valid_version || self.id.len() != 32 || !self.id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("호환되지 않는 Edolterm 실행 요청입니다.".into());
        }
        if let Some(path) = &self.directory
            && (!path.is_absolute() || !path.is_dir())
        {
            return Err(format!("시작 폴더를 찾을 수 없습니다: {}", path.display()));
        }
        if let Some(profile) = &self.profile
            && (profile.trim().is_empty() || profile.contains('\0'))
        {
            return Err("프로필 ID가 비어 있거나 올바르지 않습니다.".into());
        }
        Ok(())
    }
}

pub struct Envelope {
    pub request: Request,
    #[cfg(windows)]
    pub handoff: Option<crate::handoff::Incoming>,
    pub reply: std::sync::mpsc::SyncSender<Result<(), String>>,
}

pub fn select_window(
    behavior: WindowingBehavior,
    force_new: bool,
    // (window ID, last activation sequence, on current desktop, closing)
    windows: impl IntoIterator<Item = (u64, u64, bool, bool)>,
) -> Option<u64> {
    if force_new || behavior == WindowingBehavior::NewWindow {
        return None;
    }
    windows
        .into_iter()
        .filter(|(_, _, current, closing)| {
            !closing && (behavior == WindowingBehavior::AnyDesktop || *current)
        })
        .max_by_key(|(_, activation, _, _)| *activation)
        .map(|(id, _, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_request_reads_legacy_packets_and_preserves_profile() {
        let request = Request::new(None, false).unwrap();
        let mut legacy = serde_json::to_value(&request).unwrap();
        legacy.as_object_mut().unwrap().remove("profile");
        let legacy: Request = serde_json::from_value(legacy).unwrap();
        assert!(legacy.profile.is_none());
        let mut request = request;
        request.profile = Some("wsl:개발 Ubuntu".into());
        let restored: Request =
            serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
        assert_eq!(restored.profile, request.profile);
        restored.validate().unwrap();
        for invalid in ["", " ", "embedded\0nul"] {
            request.profile = Some(invalid.into());
            assert!(request.validate().is_err());
        }
    }
    #[test]
    fn policies_select_mru_without_crossing_desktops_or_closing_windows() {
        let windows = [
            (1, 5, true, false),
            (2, 10, false, false),
            (3, 20, true, true),
        ];
        assert_eq!(
            select_window(WindowingBehavior::CurrentDesktop, false, windows),
            Some(1)
        );
        assert_eq!(
            select_window(WindowingBehavior::AnyDesktop, false, windows),
            Some(2)
        );
        assert_eq!(
            select_window(WindowingBehavior::NewWindow, false, windows),
            None
        );
        assert_eq!(
            select_window(WindowingBehavior::CurrentDesktop, true, windows),
            None
        );
        assert_eq!(
            select_window(
                WindowingBehavior::CurrentDesktop,
                false,
                [(2, 10, false, false)]
            ),
            None
        );
        assert_eq!(
            select_window(WindowingBehavior::AnyDesktop, false, []),
            None
        );
    }
}
