//! GitHub release discovery and a saved-data-first updater handoff on each OS.
use anyhow::{Result, anyhow};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{Handoff, install_scope, prepare};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("EDOLTERM_RELEASE_REPOSITORY");
const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
pub const RESULT_FILE: &str = "update-result.txt";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Version([u16; 3]);

impl Version {
    fn parse(text: &str) -> Result<Self> {
        let mut parts = text.split('.');
        let mut numbers = [0; 3];
        for number in &mut numbers {
            let part = parts.next().ok_or_else(|| anyhow!("잘못된 버전입니다"))?;
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return Err(anyhow!("잘못된 버전입니다"));
            }
            *number = part.parse()?;
        }
        if parts.next().is_some() {
            return Err(anyhow!("잘못된 버전입니다"));
        }
        Ok(Self(numbers))
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub size: u64,
    pub sha256: String,
    pub asset_name: String,
}

#[derive(serde::Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

#[derive(serde::Deserialize)]
struct ApiAsset {
    name: String,
    size: u64,
    digest: Option<String>,
}

impl Release {
    fn from_api(response: ApiRelease, package: &str) -> Result<Self> {
        let version = response
            .tag_name
            .strip_prefix('v')
            .ok_or_else(|| anyhow!("잘못된 릴리스 태그입니다"))?
            .to_owned();
        Version::parse(&version)?;
        let asset = response
            .assets
            .into_iter()
            .find(|asset| asset.name == package)
            .ok_or_else(|| anyhow!("현재 플랫폼의 업데이트 파일이 없습니다"))?;
        let sha256 = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .ok_or_else(|| anyhow!("업데이트 파일의 SHA-256 정보가 없습니다"))?
            .to_owned();
        if response.draft
            || response.prerelease
            || asset.size == 0
            || asset.size > MAX_INSTALLER_BYTES
            || sha256.len() != 64
            || !sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(anyhow!("잘못된 업데이트 정보입니다"));
        }
        Ok(Self {
            version,
            size: asset.size,
            sha256: sha256.to_ascii_lowercase(),
            asset_name: asset.name,
        })
    }

    fn installer_url(&self) -> String {
        format!(
            "https://github.com/{REPOSITORY}/releases/download/v{}/{}",
            self.version, self.asset_name
        )
    }
}

fn package_name(exe: &Path) -> &'static str {
    #[cfg(windows)]
    {
        if install_scope(exe).is_some() {
            "edolterm-installer-exe.exe"
        } else {
            "edolterm-windows.zip"
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        "edolterm-windows.zip"
    }
}

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Unchecked,
    Checking,
    Current,
    NoRelease,
    Available(Release),
    Installing,
    Ready,
    Failed(String),
}

impl State {
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing)
    }
}

pub fn check() -> Result<State> {
    let mut bytes = Vec::new();
    let status = get(
        &format!("https://api.github.com/repos/{REPOSITORY}/releases/latest"),
        1024 * 1024,
        Duration::from_secs(20),
        &mut bytes,
    )?;
    if status == 404 {
        return Ok(State::NoRelease);
    }
    if status != 200 {
        return Err(anyhow!("업데이트 서버 응답: {status}"));
    }
    let response: ApiRelease = serde_json::from_slice(&bytes)?;
    if response.draft || response.prerelease {
        return Err(anyhow!("정식 릴리스가 아닙니다"));
    }
    let version = response
        .tag_name
        .strip_prefix('v')
        .ok_or_else(|| anyhow!("잘못된 릴리스 태그입니다"))?;
    if Version::parse(version)? <= Version::parse(VERSION)? {
        return Ok(State::Current);
    }
    let exe = std::env::current_exe()?;
    Ok(State::Available(Release::from_api(
        response,
        package_name(&exe),
    )?))
}

#[cfg(windows)]
fn get(address: &str, limit: usize, time: Duration, output: &mut impl Write) -> Result<u16> {
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::Networking::WinHttp::*;
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
    fn handle(raw: *mut c_void) -> Result<Handle> {
        if raw.is_null() {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(Handle(raw))
        }
    }
    fn succeeded(ok: i32) -> Result<()> {
        if ok == 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let url = url::Url::parse(address)?;
    if url.scheme() != "https" || url.port_or_known_default() != Some(443) {
        return Err(anyhow!("HTTPS is required"));
    }
    let host = wide(url.host_str().ok_or_else(|| anyhow!("Missing host"))?);
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let path = wide(&path);
    let agent = wide(concat!("Edolterm/", env!("CARGO_PKG_VERSION")));
    let get = wide("GET");
    let headers = wide("Accept: application/vnd.github+json\r\n");
    // Every native handle is owned above; buffers and UTF-16 strings outlive each call.
    unsafe {
        let session = handle(WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        ))?;
        let timeout = time.as_millis().min(i32::MAX as u128) as i32;
        succeeded(WinHttpSetTimeouts(
            session.0, timeout, timeout, timeout, timeout,
        ))?;
        let connection = handle(WinHttpConnect(session.0, host.as_ptr(), 443, 0))?;
        let request = handle(WinHttpOpenRequest(
            connection.0,
            get.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        succeeded(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            ptr::null(),
            0,
            0,
            0,
        ))?;
        succeeded(WinHttpReceiveResponse(request.0, ptr::null_mut()))?;
        let mut status = 0u32;
        let mut size = 4u32;
        succeeded(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            ptr::null_mut(),
        ))?;
        let mut buffer = [0u8; 65536];
        let mut total = 0usize;
        let start = std::time::Instant::now();
        loop {
            let mut read = 0;
            succeeded(WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as usize)
                .ok_or_else(|| anyhow!("Download size overflow"))?;
            if total > limit || start.elapsed() > time {
                return Err(anyhow!("Download limit exceeded"));
            }
            output.write_all(&buffer[..read as usize])?;
        }
        Ok(status as u16)
    }
}
#[cfg(not(windows))]
fn get(_: &str, _: usize, _: Duration, _: &mut impl Write) -> Result<u16> {
    Err(anyhow!("Updates are supported on Windows x64 only"))
}

pub fn read_result(config: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(config.join(RESULT_FILE))
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    let text = text.trim();
    // Older helpers persisted a success notice. Keep errors visible without showing that banner.
    if text.is_empty()
        || text
            .strip_prefix('v')
            .and_then(|text| text.strip_suffix(" 업데이트를 완료했습니다."))
            .is_some_and(|version| Version::parse(version).is_ok())
    {
        return None;
    }
    Some(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "uses the public GitHub API"]
    fn live_github_release_check() {
        assert!(matches!(
            check().unwrap(),
            State::Current | State::NoRelease | State::Available(_)
        ));
    }
    #[test]
    fn versions_are_numeric_and_strict() {
        assert!(Version::parse("1.10.0").unwrap() > Version::parse("1.9.9").unwrap());
        for invalid in ["01.1.0", "1.2", "1.2.3.4", "1.2.3-beta", "65536.0.0"] {
            assert!(Version::parse(invalid).is_err());
        }
    }
    #[test]
    fn release_requires_bounded_asset_and_sha256() {
        let response = |digest, size| ApiRelease {
            tag_name: "v1.2.3".into(),
            draft: false,
            prerelease: false,
            assets: vec![ApiAsset {
                name: "edolterm-windows.zip".into(),
                size,
                digest,
            }],
        };
        assert!(
            Release::from_api(
                response(Some(format!("sha256:{}", "ab".repeat(32))), 1024),
                "edolterm-windows.zip"
            )
            .is_ok()
        );
        assert!(Release::from_api(response(None, 1024), "edolterm-windows.zip").is_err());
        assert!(
            Release::from_api(
                response(
                    Some(format!("sha256:{}", "ab".repeat(32))),
                    MAX_INSTALLER_BYTES + 1
                ),
                "edolterm-windows.zip"
            )
            .is_err()
        );
    }
}
