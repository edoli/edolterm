//! Per-logon, per-config, per-integrity-level single GUI host with bounded IPC.
use super::{Envelope, Request};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    ffi::OsStr,
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        *,
    },
    Storage::FileSystem::*,
    System::{IO::*, Pipes::*, Threading::*},
    UI::WindowsAndMessaging::AllowSetForegroundWindow,
};

const MAX_PACKET: usize = 256 * 1024;
const IO_TIMEOUT: u32 = 10000;

#[derive(Serialize, Deserialize)]
struct Reply {
    error: Option<String>,
}

pub enum Outcome {
    Host(Host),
    Forwarded,
}
pub struct Host {
    pub requests: async_channel::Receiver<Envelope>,
    _lease: Lease,
}
struct Lease {
    mutex: OwnedHandle,
    stop: Arc<OwnedHandle>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe {
            SetEvent(self.stop.as_raw_handle());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe {
            ReleaseMutex(self.mutex.as_raw_handle());
        }
    }
}
fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}
fn event() -> io::Result<OwnedHandle> {
    owned(unsafe { CreateEventW(null(), 1, 0, null()) })
}
struct LocalMemory(*mut std::ffi::c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<usize>> {
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, class, null_mut(), 0, &mut needed);
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

#[derive(Clone)]
struct Scope {
    mutex: Vec<u16>,
    pipe: Vec<u16>,
    sddl: Vec<u16>,
}
impl Scope {
    fn new(dir: &Path) -> Result<Self> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let token = owned(token)?;
        let groups = token_info(token.as_raw_handle(), TokenLogonSid)?;
        let groups = unsafe { &*groups.as_ptr().cast::<TOKEN_GROUPS>() };
        if groups.GroupCount != 1 {
            bail!("로그인 세션을 확인할 수 없습니다.");
        }
        let mut sid_string = null_mut();
        if unsafe { ConvertSidToStringSidW(groups.Groups[0].Sid, &mut sid_string) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let sid_allocation = LocalMemory(sid_string.cast());
        let mut length = 0;
        unsafe {
            while *sid_string.add(length) != 0 {
                length += 1;
            }
        }
        let sid =
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid_string, length) });
        drop(sid_allocation);
        let elevation = token_info(token.as_raw_handle(), TokenElevation)?;
        let elevation = unsafe { &*elevation.as_ptr().cast::<TOKEN_ELEVATION>() }.TokenIsElevated;
        std::fs::create_dir_all(dir)?;
        let canonical = dir.canonicalize()?;
        let mut hash = 0xcbf29ce484222325u64;
        for word in canonical.as_os_str().encode_wide() {
            for byte in word.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        let name = format!("Edolterm-v1-{sid}-{elevation}-{hash:016x}");
        Ok(Self {
            mutex: wide(format!("Local\\{name}")),
            pipe: wide(format!(r"\\.\pipe\{name}")),
            sddl: wide(format!("D:P(A;;GA;;;{sid})")),
        })
    }
    fn security(&self) -> io::Result<(LocalMemory, SECURITY_ATTRIBUTES)> {
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                self.sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((
            LocalMemory(descriptor),
            SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            },
        ))
    }
    fn pipe(&self) -> io::Result<OwnedHandle> {
        let (_descriptor, security) = self.security()?;
        owned(unsafe {
            CreateNamedPipeW(
                self.pipe.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                IO_TIMEOUT,
                &security,
            )
        })
    }
}

fn wait_io(
    pipe: HANDLE,
    operation: &OVERLAPPED,
    stop: HANDLE,
    pending: bool,
    timeout: u32,
) -> io::Result<u32> {
    if pending {
        let handles = [stop, operation.hEvent];
        let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, timeout) };
        if wait != WAIT_OBJECT_0 + 1 {
            // Drain cancellation before the OVERLAPPED or its buffer leaves scope.
            unsafe {
                CancelIoEx(pipe, operation);
            }
            let mut ignored = 0;
            unsafe {
                GetOverlappedResult(pipe, operation, &mut ignored, 1);
            }
            return Err(io::Error::new(
                if wait == WAIT_TIMEOUT {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                },
                "pipe operation canceled or timed out",
            ));
        }
    }
    let mut transferred = 0;
    if unsafe { GetOverlappedResult(pipe, operation, &mut transferred, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(transferred)
}
fn connect(pipe: HANDLE, stop: HANDLE) -> io::Result<()> {
    let ready = event()?;
    let mut operation: OVERLAPPED = unsafe { zeroed() };
    operation.hEvent = ready.as_raw_handle();
    if unsafe { ConnectNamedPipe(pipe, &mut operation) } != 0 {
        return Ok(());
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_PIPE_CONNECTED {
        return Ok(());
    }
    if error != ERROR_IO_PENDING {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    // A listening host can remain idle indefinitely; the stop event cancels it.
    wait_io(pipe, &operation, stop, true, INFINITE).map(|_| ())
}
fn transfer(pipe: HANDLE, buffer: &mut [u8], writing: bool, stop: HANDLE) -> io::Result<()> {
    let mut offset = 0;
    while offset < buffer.len() {
        let ready = event()?;
        let mut operation: OVERLAPPED = unsafe { zeroed() };
        operation.hEvent = ready.as_raw_handle();
        let size = (buffer.len() - offset).min(65536) as u32;
        let mut ignored = 0;
        let result = unsafe {
            if writing {
                WriteFile(
                    pipe,
                    buffer[offset..].as_ptr(),
                    size,
                    &mut ignored,
                    &mut operation,
                )
            } else {
                ReadFile(
                    pipe,
                    buffer[offset..].as_mut_ptr(),
                    size,
                    &mut ignored,
                    &mut operation,
                )
            }
        };
        let pending = result == 0;
        if pending && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        let count = wait_io(pipe, &operation, stop, pending, IO_TIMEOUT)? as usize;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        offset += count;
    }
    Ok(())
}
fn send<T: Serialize>(pipe: HANDLE, value: &T, stop: HANDLE) -> Result<()> {
    let mut body = serde_json::to_vec(value)?;
    if body.len() > MAX_PACKET {
        bail!("실행 요청이 너무 큽니다.");
    }
    transfer(pipe, &mut (body.len() as u32).to_le_bytes(), true, stop)?;
    transfer(pipe, &mut body, true, stop)?;
    Ok(())
}
fn receive<T: DeserializeOwned>(pipe: HANDLE, stop: HANDLE) -> Result<T> {
    let mut length = [0; 4];
    transfer(pipe, &mut length, false, stop)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_PACKET {
        bail!("잘못된 실행 요청 크기입니다.");
    }
    let mut body = vec![0; length];
    transfer(pipe, &mut body, false, stop)?;
    Ok(serde_json::from_slice(&body)?)
}

fn respond(pipe: HANDLE, error: Option<String>, stop: HANDLE) -> Result<()> {
    send(pipe, &Reply { error }, stop)?;
    // DisconnectNamedPipe discards unread bytes. Wait for the client to consume
    // the reply without a potentially unbounded FlushFileBuffers call.
    transfer(pipe, &mut [0], false, stop)?;
    Ok(())
}

pub fn start(dir: &Path, request: Request) -> Result<Outcome> {
    let scope = Scope::new(dir)?;
    let (_descriptor, security) = scope.security()?;
    let mutex = owned(unsafe { CreateMutexW(&security, 0, scope.mutex.as_ptr()) })?;
    let began = Instant::now();
    loop {
        let ownership = unsafe { WaitForSingleObject(mutex.as_raw_handle(), 0) };
        if ownership == WAIT_OBJECT_0 || ownership == WAIT_ABANDONED {
            let pipe = match scope.pipe() {
                Ok(pipe) => pipe,
                Err(error) => {
                    unsafe {
                        ReleaseMutex(mutex.as_raw_handle());
                    }
                    return Err(error.into());
                }
            };
            let stop = Arc::new(event()?);
            let (tx, requests) = async_channel::bounded(32);
            let stopping = stop.clone();
            let worker = thread::Builder::new()
                .name("edolterm-launch-ipc".into())
                .spawn(move || {
                    let mut next_pipe = Some(pipe);
                    while unsafe { WaitForSingleObject(stopping.as_raw_handle(), 0) }
                        == WAIT_TIMEOUT
                    {
                        let pipe = match next_pipe.take().map(Ok).unwrap_or_else(|| scope.pipe()) {
                            Ok(pipe) => pipe,
                            Err(_) => break,
                        };
                        let handle = pipe.as_raw_handle();
                        if connect(handle, stopping.as_raw_handle()).is_err() {
                            // A client can exit between CreateFile and ConnectNamedPipe.
                            // Recreate the listener; the stop event ends the outer loop.
                            continue;
                        }
                        let result = (|| -> Result<()> {
                            let request: Request = receive(handle, stopping.as_raw_handle())?;
                            if let Err(error) = request.validate() {
                                respond(handle, Some(error), stopping.as_raw_handle())?;
                                return Ok(());
                            }
                            let (reply, result) = mpsc::sync_channel(1);
                            // Only duplicate handles from the actual pipe client, never a PID
                            // selected by the packet. The source retains ownership until reply.
                            let handoff = request
                                .handoff
                                .as_ref()
                                .map(|descriptor| {
                                    let mut peer = 0;
                                    if unsafe { GetNamedPipeClientProcessId(handle, &mut peer) }
                                        == 0
                                    {
                                        return Err(io::Error::last_os_error().into());
                                    }
                                    crate::handoff::Incoming::duplicate(descriptor, peer)
                                })
                                .transpose()?;
                            tx.try_send(Envelope {
                                request,
                                handoff,
                                reply,
                            })
                            .map_err(|_| anyhow!("GUI host stopped"))?;
                            let began = Instant::now();
                            let response = loop {
                                match result.recv_timeout(Duration::from_millis(50)) {
                                    Ok(result) => break result,
                                    Err(mpsc::RecvTimeoutError::Timeout)
                                        if began.elapsed() < Duration::from_secs(8)
                                            && unsafe {
                                                WaitForSingleObject(stopping.as_raw_handle(), 0)
                                            } == WAIT_TIMEOUT => {}
                                    _ => bail!("GUI host did not acknowledge launch request"),
                                }
                            };
                            respond(handle, response.err(), stopping.as_raw_handle())
                        })();
                        let _ = result;
                        unsafe {
                            DisconnectNamedPipe(handle);
                        }
                    }
                })?;
            return Ok(Outcome::Host(Host {
                requests,
                _lease: Lease {
                    mutex,
                    stop,
                    worker: Some(worker),
                },
            }));
        }
        if ownership != WAIT_TIMEOUT {
            bail!(
                "실행 중인 Edolterm 확인 실패: {}",
                io::Error::last_os_error()
            );
        }
        let pipe = unsafe {
            CreateFileW(
                scope.pipe.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if pipe != INVALID_HANDLE_VALUE {
            let pipe = owned(pipe)?;
            let mut server = 0;
            if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut server) } == 0 {
                return Err(io::Error::last_os_error().into());
            }
            unsafe {
                AllowSetForegroundWindow(server);
            }
            let stop = event()?;
            send(pipe.as_raw_handle(), &request, stop.as_raw_handle())?;
            let reply: Reply = receive(pipe.as_raw_handle(), stop.as_raw_handle()).context(
                "실행 중인 Edolterm이 요청에 응답하지 않습니다. 잠시 후 다시 시도하세요.",
            )?;
            transfer(pipe.as_raw_handle(), &mut [1], true, stop.as_raw_handle())?;
            if let Some(error) = reply.error {
                bail!(error);
            }
            return Ok(Outcome::Forwarded);
        }
        let error = io::Error::last_os_error();
        if !matches!(
            error.raw_os_error().map(|code| code as u32),
            Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY)
        ) {
            return Err(error.into());
        }
        if began.elapsed() > Duration::from_secs(8) {
            bail!("Edolterm이 시작 또는 종료 중입니다. 잠시 후 다시 시도하세요.");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_round_trip_concurrent_clients_errors_and_host_recovery() {
        let dir = std::env::temp_dir().join(format!("edolterm-pipe-test-{}", std::process::id()));
        let initial = Request::new(None, false).unwrap();
        let Outcome::Host(host) = start(&dir, initial.clone()).unwrap() else {
            panic!("expected host")
        };
        // Aborted and malformed launchers cannot permanently stop the listener.
        let probe_dir = dir.clone();
        thread::spawn(move || {
            let scope = Scope::new(&probe_dir).unwrap();
            for malformed in [false, true] {
                let began = Instant::now();
                let pipe = loop {
                    let handle = unsafe {
                        CreateFileW(
                            scope.pipe.as_ptr(),
                            GENERIC_READ | GENERIC_WRITE,
                            0,
                            null(),
                            OPEN_EXISTING,
                            FILE_FLAG_OVERLAPPED,
                            null_mut(),
                        )
                    };
                    if let Ok(pipe) = owned(handle) {
                        break pipe;
                    }
                    assert!(began.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(5));
                };
                if malformed {
                    let stop = event().unwrap();
                    transfer(
                        pipe.as_raw_handle(),
                        &mut (MAX_PACKET as u32 + 1).to_le_bytes(),
                        true,
                        stop.as_raw_handle(),
                    )
                    .unwrap();
                }
            }
        })
        .join()
        .unwrap();
        let mut clients = Vec::new();
        for i in 0..6 {
            let dir = dir.clone();
            clients.push(thread::spawn(move || {
                let mut request = Request::new(Some(dir.clone()), i == 4).unwrap();
                request.profile = Some("wsl:개발 Ubuntu".into());
                if i == 5 {
                    request.version = 999;
                }
                let response = start(&dir, request);
                if i >= 4 {
                    assert!(response.is_err());
                } else {
                    assert!(matches!(response.unwrap(), Outcome::Forwarded));
                }
            }));
        }
        for _ in 0..5 {
            let began = Instant::now();
            let envelope = loop {
                if let Ok(envelope) = host.requests.try_recv() {
                    break envelope;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(15),
                    "pipe request timed out"
                );
                thread::sleep(Duration::from_millis(5));
            };
            envelope.request.validate().unwrap();
            assert_eq!(envelope.request.profile.as_deref(), Some("wsl:개발 Ubuntu"));
            // One GUI rejection must reach the launch client intact.
            let response = if envelope.request.new_window {
                Err("test GUI rejection".into())
            } else {
                Ok(())
            };
            envelope.reply.send(response).unwrap();
        }
        for client in clients {
            client.join().unwrap();
        }
        drop(host);
        assert!(matches!(start(&dir, initial).unwrap(), Outcome::Host(_)));
        std::fs::remove_dir(&dir).unwrap();
    }
}
