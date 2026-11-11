//! Windows default-terminal connections. Handles are inherited from the COM
//! endpoint or duplicated from the authenticated instance-pipe client.
use alacritty_terminal::{
    event::{OnResize, WindowSize},
    tty::{ChildEvent, EventedPty, EventedReadWrite},
};
use anyhow::{Context, Result, bail};
use polling::{
    Event, PollMode, Poller,
    os::iocp::{CompletionPacket, PollerIocpExt},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle},
    },
    sync::{Arc, OnceLock},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        Console::{COORD, HPCON},
        LibraryLoader::*,
        Threading::*,
    },
};

#[path = "handoff/blocking.rs"]
mod blocking;
const IO_TOKEN: usize = alacritty_terminal::tty::windows::PTY_READ_WRITE_TOKEN;
const EXIT_TOKEN: usize = alacritty_terminal::tty::windows::PTY_CHILD_EVENT_TOKEN;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Descriptor {
    pub pid: u32,
    pub handles: [usize; 7],
    pub title: String,
}

impl Descriptor {
    pub fn validate(&self) -> Result<()> {
        if self.pid == 0 || self.title.len() > 32768 {
            bail!("잘못된 콘솔 연결 요청입니다.");
        }
        validate_values(&self.handles)
    }
}

pub struct Incoming {
    // input writer, output reader, signal, reference, server, client, ready event
    handles: [OwnedHandle; 7],
    pub title: String,
}

impl Incoming {
    pub fn program(&self) -> Option<String> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe {
            QueryFullProcessImageNameW(
                self.handles[5].as_raw_handle(),
                0,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
        {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }
    pub fn inherit(text: &str) -> Result<Self> {
        let values = parse_handles(text)?;
        // Validate the complete packet before taking ownership of any handle.
        for value in values {
            let mut flags = 0;
            if unsafe { GetHandleInformation(value as HANDLE, &mut flags) } == 0
                || flags & HANDLE_FLAG_INHERIT == 0
            {
                bail!("상속된 콘솔 핸들이 올바르지 않습니다.");
            }
        }
        let handles = values.map(|value| unsafe { OwnedHandle::from_raw_handle(value as _) });
        for handle in &handles {
            if unsafe { SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0
            {
                return Err(io::Error::last_os_error().into());
            }
        }
        Ok(Self {
            handles,
            title: "외부 콘솔".into(),
        })
    }

    pub fn descriptor(&self) -> Descriptor {
        Descriptor {
            pid: std::process::id(),
            handles: std::array::from_fn(|i| self.handles[i].as_raw_handle() as usize),
            title: self.title.clone(),
        }
    }

    pub fn duplicate(descriptor: &Descriptor, peer: u32) -> Result<Self> {
        if descriptor.pid != peer || descriptor.title.len() > 32768 {
            bail!("콘솔 연결 요청의 발신자가 일치하지 않습니다.");
        }
        descriptor.validate()?;
        let source = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, peer) };
        if source.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        let source = unsafe { OwnedHandle::from_raw_handle(source) };
        let mut handles = Vec::new();
        for value in descriptor.handles {
            let mut target = std::ptr::null_mut();
            if unsafe {
                DuplicateHandle(
                    source.as_raw_handle(),
                    value as HANDLE,
                    GetCurrentProcess(),
                    &mut target,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(io::Error::last_os_error().into());
            }
            handles.push(unsafe { OwnedHandle::from_raw_handle(target) });
        }
        Ok(Self {
            handles: handles.try_into().ok().unwrap(),
            title: descriptor.title.clone(),
        })
    }
}

fn validate_values(values: &[usize; 7]) -> Result<()> {
    for (i, value) in values.iter().enumerate() {
        if *value == 0 || *value >= isize::MAX as usize || values[..i].contains(value) {
            bail!("콘솔 핸들이 비어 있거나 중복되었습니다.");
        }
    }
    Ok(())
}
fn parse_handles(text: &str) -> Result<[usize; 7]> {
    let values: Vec<usize> = text
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let values: [usize; 7] = values
        .try_into()
        .map_err(|_| anyhow::anyhow!("콘솔 핸들 개수가 올바르지 않습니다."))?;
    validate_values(&values)?;
    Ok(values)
}

type Pack = unsafe extern "system" fn(HANDLE, HANDLE, HANDLE, *mut HPCON) -> i32;
type Resize = unsafe extern "system" fn(HPCON, COORD) -> i32;
type Release = unsafe extern "system" fn(HPCON) -> i32;
type Close = unsafe extern "system" fn(HPCON);
struct Api {
    pack: Pack,
    resize: Resize,
    release: Release,
    close: Close,
}
fn api() -> Result<&'static Api> {
    static API: OnceLock<std::result::Result<Api, String>> = OnceLock::new();
    API.get_or_init(|| {
        (|| -> Result<Api> {
            let path = std::env::current_exe()?.with_file_name("conpty.dll");
            let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // Keep the module loaded for every connection's entire lifetime.
            let module = unsafe {
                LoadLibraryExW(
                    path.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
            if module.is_null() {
                return Err(io::Error::last_os_error())
                    .context("기본 터미널에는 함께 배포된 conpty.dll이 필요합니다");
            }
            unsafe {
                Ok(Api {
                    pack: std::mem::transmute::<unsafe extern "system" fn() -> isize, Pack>(
                        GetProcAddress(module, c"ConptyPackPseudoConsole".as_ptr().cast())
                            .context("ConptyPackPseudoConsole")?,
                    ),
                    resize: std::mem::transmute::<unsafe extern "system" fn() -> isize, Resize>(
                        GetProcAddress(module, c"ConptyResizePseudoConsole".as_ptr().cast())
                            .context("ConptyResizePseudoConsole")?,
                    ),
                    release: std::mem::transmute::<unsafe extern "system" fn() -> isize, Release>(
                        GetProcAddress(module, c"ConptyReleasePseudoConsole".as_ptr().cast())
                            .context("ConptyReleasePseudoConsole")?,
                    ),
                    close: std::mem::transmute::<unsafe extern "system" fn() -> isize, Close>(
                        GetProcAddress(module, c"ConptyClosePseudoConsole".as_ptr().cast())
                            .context("ConptyClosePseudoConsole")?,
                    ),
                })
            }
        })()
        .map_err(|e| format!("{e:#}"))
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!(e.clone()))
}

struct Console {
    handle: HPCON,
    api: &'static Api,
}
unsafe impl Send for Console {}
impl Drop for Console {
    fn drop(&mut self) {
        let handle = self.handle;
        let close = self.api.close;
        // Closing can wait for output. Release the bounded pipe adapters while
        // it runs, so a full output buffer cannot deadlock a closed tab.
        std::thread::spawn(move || unsafe { close(handle) });
    }
}

pub fn change_default(restore: bool) -> Result<String> {
    use std::os::windows::process::CommandExt;
    let helper = std::env::current_exe()?
        .parent()
        .context("실행 경로")?
        .join("shell-integration/windows-default-terminal.ps1");
    if !helper.is_file() {
        bail!("Edolterm 설치형을 설치한 뒤 사용하세요.");
    }
    let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let output = std::process::Command::new(shell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(helper)
        .args(["-Action", if restore { "Restore" } else { "Set" }])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub struct Pty {
    // ClosePseudoConsole must run while the output reader still drains its pipe.
    console: Console,
    output: blocking::UnblockedReader<File>,
    input: blocking::UnblockedWriter<File>,
    _client: OwnedHandle,
    ready: Option<OwnedHandle>,
    poll: Option<Arc<Poller>>,
}
impl Pty {
    pub fn new(incoming: Incoming) -> Result<Self> {
        let api = api()?;
        let [input, output, signal, reference, server, client, ready] = incoming.handles;
        let mut handle = 0;
        let result = unsafe {
            (api.pack)(
                server.as_raw_handle(),
                reference.as_raw_handle(),
                signal.as_raw_handle(),
                &mut handle,
            )
        };
        if result < 0 {
            bail!("콘솔 연결 실패: 0x{:08x}", result as u32);
        }
        // Pack takes ownership on success.
        let _ = (
            server.into_raw_handle(),
            reference.into_raw_handle(),
            signal.into_raw_handle(),
        );
        let console = Console { handle, api };
        // Release our reference so the host exits after the LAST client exits.
        // Watching the first client alone would close consoles with live children.
        if unsafe { (api.release)(handle) } < 0 {
            bail!("콘솔 수명 연결 실패");
        }
        Ok(Self {
            console,
            output: blocking::UnblockedReader::new(File::from(output), 1024 * 1024),
            input: blocking::UnblockedWriter::new(File::from(input), 1024 * 1024),
            _client: client,
            ready: Some(ready),
            poll: None,
        })
    }
    fn check_exit(&self) {
        if self.output.exhausted()
            && let Some(poll) = &self.poll
        {
            let _ = poll.post(CompletionPacket::new(Event::readable(EXIT_TOKEN)));
        }
    }
}
impl Read for Pty {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.output.read(bytes)?;
        self.check_exit();
        Ok(count)
    }
}
impl Write for Pty {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.input.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.input.flush()
    }
}
impl EventedReadWrite for Pty {
    type Reader = Self;
    type Writer = Self;
    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        self.poll = Some(poll.clone());
        self.reregister(poll, event, mode)?;
        if let Some(ready) = self.ready.take() {
            unsafe {
                SetEvent(ready.as_raw_handle());
            }
        }
        Ok(())
    }
    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        event.key = IO_TOKEN;
        self.output.register(poll, event, mode);
        self.input.register(poll, event, mode);
        self.check_exit();
        Ok(())
    }
    fn deregister(&mut self, _: &Arc<Poller>) -> io::Result<()> {
        self.output.deregister();
        self.input.deregister();
        self.poll = None;
        Ok(())
    }
    fn reader(&mut self) -> &mut Self {
        self
    }
    fn writer(&mut self) -> &mut Self {
        self
    }
}
impl EventedPty for Pty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        use std::os::windows::process::ExitStatusExt;
        if !self.output.exhausted() {
            return None;
        }
        let mut code = 0;
        let status = (unsafe { GetExitCodeProcess(self._client.as_raw_handle(), &mut code) } != 0
            && code != 259)
            .then(|| std::process::ExitStatus::from_raw(code));
        Some(ChildEvent::Exited(status))
    }
}
impl OnResize for Pty {
    fn on_resize(&mut self, size: WindowSize) {
        unsafe {
            (self.console.api.resize)(
                self.console.handle,
                COORD {
                    X: size.num_cols.clamp(1, i16::MAX as u16) as i16,
                    Y: size.num_lines.clamp(1, i16::MAX as u16) as i16,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicated_connection_handles_outlive_the_forwarding_launcher() {
        let handles = std::array::from_fn(|_| {
            let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            assert!(!handle.is_null());
            unsafe { OwnedHandle::from_raw_handle(handle) }
        });
        let original = Incoming {
            handles,
            title: "한글 \"title\" \\".into(),
        };
        let forwarded = Incoming::duplicate(&original.descriptor(), std::process::id()).unwrap();
        drop(original);
        assert_eq!(unsafe { SetEvent(forwarded.handles[6].as_raw_handle()) }, 1);
        assert_eq!(
            unsafe { WaitForSingleObject(forwarded.handles[6].as_raw_handle(), 0) },
            WAIT_OBJECT_0
        );
        for handle in &forwarded.handles {
            let mut flags = u32::MAX;
            assert_ne!(
                unsafe { GetHandleInformation(handle.as_raw_handle(), &mut flags) },
                0
            );
            assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
        }
    }
    #[test]
    fn handle_packets_reject_duplicates_pseudo_handles_and_wrong_counts() {
        for invalid in [
            "",
            "1,2,3",
            "1,2,3,4,5,6,0",
            "1,2,3,4,5,6,6",
            "1,2,3,4,5,6,18446744073709551615",
        ] {
            assert!(parse_handles(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            parse_handles("4,8,12,16,20,24,28").unwrap(),
            [4, 8, 12, 16, 20, 24, 28]
        );
    }
    #[test]
    fn ipc_cannot_duplicate_from_an_unrelated_process() {
        let packet = Descriptor {
            pid: std::process::id(),
            handles: [4, 8, 12, 16, 20, 24, 28],
            title: String::new(),
        };
        assert!(Incoming::duplicate(&packet, packet.pid + 1).is_err());
    }
}
