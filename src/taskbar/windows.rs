use super::APP_ID;
mod bitmap;
mod cache;
mod executable;
use crate::{config::Profile, platform};
use cache::{IconCache, Task};
use std::{
    path::Path,
    sync::{OnceLock, mpsc},
    thread,
};
use windows::{
    Win32::{
        Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Title},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, StructuredStorage::PROPVARIANT,
        },
        UI::Shell::{
            Common::{IObjectArray, IObjectCollection},
            DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
            PropertiesSystem::IPropertyStore,
            ShellLink,
        },
    },
    core::{HSTRING, Interface, Result},
};

struct Request {
    profiles: Vec<Profile>,
    publish: bool,
    reply: async_channel::Sender<Vec<crate::icons::profiles::LoadedIcon>>,
}

pub(super) fn schedule(
    profiles: Vec<Profile>,
    directory: &Path,
    publish: bool,
) -> async_channel::Receiver<Vec<crate::icons::profiles::LoadedIcon>> {
    static WORKER: OnceLock<Option<mpsc::Sender<Request>>> = OnceLock::new();
    let (reply, images) = async_channel::bounded(1);
    let sender = WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Request>();
        let cache_path = directory.join("cache/taskbar-icons.json");
        match thread::Builder::new()
            .name("edolterm-jump-list".into())
            .spawn(move || {
                // Shell COM objects stay on one dedicated STA thread. Serial updates
                // prevent an older settings snapshot overwriting a newer one.
                if let Err(error) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() } {
                    eprintln!("{}", crate::tf!("jump_list_com_initialization_failed_error", error => error));
                    return;
                }
                let mut cache = IconCache::load(&cache_path);
                let mut bitmaps = bitmap::BitmapCache::default();
                while let Ok(mut request) = receiver.recv() {
                    for latest in receiver.try_iter() {
                        request = latest;
                    }
                    if let Err(error) = process(&request, &mut cache, &mut bitmaps) {
                        eprintln!("{}", crate::tf!("failed_to_update_jump_list_error", error => format!("{:#}", error)));
                    }
                    if let Err(error) = cache.save(&cache_path) {
                        eprintln!("{}", crate::tf!("failed_to_save_jump_list_icon_cache_error", error => format!("{:#}", error)));
                    }
                }
                unsafe { CoUninitialize() };
            }) {
            Ok(_) => Some(sender),
            Err(error) => {
                eprintln!("{}", crate::tf!("failed_to_start_jump_list_thread_error", error => error));
                None
            }
        }
    });
    if let Some(sender) = sender {
        let _ = sender.send(Request {
            profiles,
            publish,
            reply,
        });
    }
    images
}

fn process(
    request: &Request,
    cache: &mut IconCache,
    bitmaps: &mut bitmap::BitmapCache,
) -> anyhow::Result<()> {
    let executable = std::env::current_exe()?;
    let tasks = cache.tasks(&request.profiles, &executable);
    // UI images are delivered even if Windows rejects the Jump List update.
    let _ = request.reply.try_send(bitmaps.load(&tasks));
    if request.publish {
        cache.publish_if_changed(&executable, &tasks, || {
            publish_list(APP_ID, &executable, &tasks)
        })?;
    }
    Ok(())
}

fn publish_list(app_id: &str, executable: &Path, entries: &[Task]) -> anyhow::Result<()> {
    // SAFETY: this worker has initialized COM; all interfaces are local to it
    // and HSTRING/PROPVARIANT values live through each synchronous COM call.
    unsafe {
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        list.SetAppID(&HSTRING::from(app_id))?;
        let mut slots = 0;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        // User Tasks cannot be individually removed. No custom destinations
        // are added, so the removed-destinations array does not apply here.
        let result = (|| -> Result<()> {
            let tasks: IObjectCollection =
                CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            for task in entries {
                let link = profile_link(app_id, executable, task)?;
                tasks.AddObject(&link)?;
            }
            if !entries.is_empty() {
                let tasks: IObjectArray = tasks.cast()?;
                list.AddUserTasks(&tasks)?;
            }
            list.CommitList()
        })();
        if result.is_err() {
            let _ = list.AbortList();
        }
        result?;
    }
    Ok(())
}

fn profile_link(app_id: &str, executable: &Path, task: &Task) -> Result<IShellLinkW> {
    let arguments = format!("--profile {}", quote_argument(&task.id));
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(executable))?;
        link.SetArguments(&HSTRING::from(arguments))?;
        link.SetDescription(&HSTRING::from(task.name.as_str()))?;
        // Explorer's launch directory must not become the terminal's cwd.
        // Session startup applies the selected profile's cwd (or home).
        link.SetWorkingDirectory(&HSTRING::from(platform::home_dir().as_path()))?;
        link.SetIconLocation(&HSTRING::from(task.icon.as_path()), 0)?;
        let properties: IPropertyStore = link.cast()?;
        properties.SetValue(&PKEY_Title, &PROPVARIANT::from(task.name.as_str()))?;
        properties.SetValue(&PKEY_AppUserModel_ID, &PROPVARIANT::from(app_id))?;
        properties.Commit()?;
        Ok(link)
    }
}

/// Quote one argument for the Windows executable command-line parser. In
/// particular, double backslashes before quotes and before the closing quote.
fn quote_argument(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for c in value.chars() {
        if c == '\\' {
            backslashes += 1;
            continue;
        }
        let count = if c == '"' {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        quoted.extend(std::iter::repeat_n('\\', count));
        backslashes = 0;
        quoted.push(c);
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        UI::Shell::CommandLineToArgvW,
    };

    #[test]
    fn shell_accepts_profile_tasks_with_titles_icons_and_arguments() {
        // Use an isolated identity: this test never replaces the user's list.
        let app_id = format!("{APP_ID}.Test.{}", std::process::id());
        let executable = std::env::current_exe().unwrap();
        let profiles = [Profile::new("wsl:개발 Ubuntu", "개발 셸", "cmd.exe", &[])];
        let tasks = IconCache::default().tasks(&profiles, &executable);
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        }
        let result = (|| -> anyhow::Result<()> {
            let link =
                profile_link(&app_id, &executable, &tasks[0]).context("create profile link")?;
            let properties: IPropertyStore = link.cast()?;
            let mut arguments = [0u16; 1024];
            let mut icon = [0u16; 1024];
            let mut icon_index = -1;
            unsafe {
                assert_eq!(properties.GetValue(&PKEY_Title)?.to_string(), "개발 셸");
                assert_eq!(
                    properties.GetValue(&PKEY_AppUserModel_ID)?.to_string(),
                    app_id
                );
                link.GetArguments(&mut arguments)?;
                link.GetIconLocation(&mut icon, &mut icon_index)?;
            }
            let end = arguments.iter().position(|c| *c == 0).unwrap();
            assert_eq!(
                String::from_utf16(&arguments[..end])?,
                "--profile \"wsl:개발 Ubuntu\""
            );
            assert_ne!(icon[0], 0);
            assert_eq!(icon_index, 0);
            publish_list(&app_id, &executable, &tasks).context("publish profiles")?;
            // Replacing the list also removes tasks for deleted profiles.
            publish_list(&app_id, &executable, &[]).context("clear profiles")?;
            Ok(())
        })();
        unsafe {
            let list: ICustomDestinationList =
                CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER).unwrap();
            let _ = list.DeleteList(&HSTRING::from(app_id));
            drop(list);
            CoUninitialize();
        }
        result.unwrap();
    }

    #[test]
    fn installed_store_powershell_aliases_use_the_real_executable_icon() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW};

        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let aliases: Vec<_> = [
            Path::new(&local).join("Microsoft/WindowsApps/pwsh.exe"),
            Path::new(&local)
                .join("Microsoft/WindowsApps/Microsoft.PowerShell_8wekyb3d8bbwe/pwsh.exe"),
        ]
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
        if aliases.is_empty() {
            eprintln!("Store PowerShell is not installed; skipping live alias check");
            return;
        }
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let result = (|| -> anyhow::Result<()> {
            for alias in aliases {
                let target = executable::resolve_icon_executable(&alias)
                    .context("resolve installed PowerShell alias")?;
                assert_ne!(target, alias);
                assert!(std::fs::metadata(&target)?.len() > 0);
                let profile = Profile::new("pwsh", "PowerShell", alias.to_string_lossy(), &[]);
                let executable = std::env::current_exe()?;
                let tasks = IconCache::default().tasks(&[profile], &executable);
                let link = profile_link(APP_ID, &executable, &tasks[0])?;
                let mut location = [0u16; 32768];
                let mut index = -1;
                unsafe { link.GetIconLocation(&mut location, &mut index)? };
                let end = location.iter().position(|unit| *unit == 0).unwrap();
                assert_eq!(Path::new(&String::from_utf16(&location[..end])?), target);
                assert_eq!(index, 0);
                let target_wide: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
                let mut icon = std::ptr::null_mut();
                let mut resource = 0;
                let count = unsafe {
                    PrivateExtractIconsW(
                        target_wide.as_ptr(),
                        0,
                        32,
                        32,
                        &mut icon,
                        &mut resource,
                        1,
                        0,
                    )
                };
                let extracted = !icon.is_null();
                if extracted {
                    unsafe { DestroyIcon(icon) };
                }
                assert_eq!(
                    count,
                    1,
                    "actual executable must contain an icon: {}",
                    target.display()
                );
                assert!(extracted);
                eprintln!(
                    "Verified PowerShell icon: {} -> {}",
                    alias.display(),
                    target.display()
                );
            }
            Ok(())
        })();
        unsafe { CoUninitialize() };
        result.unwrap();
    }

    #[test]
    fn profile_arguments_round_trip_through_windows_parser() {
        for id in [
            "cmd",
            "wsl:개발 Ubuntu",
            "quote\"here",
            "trailing\\",
            "before\\\"quote",
            "& %PATH% $value;",
            "--profile=other",
        ] {
            let line = HSTRING::from(format!("edolterm.exe --profile {}", quote_argument(id)));
            let mut count = 0;
            unsafe {
                let args = CommandLineToArgvW(&line, &mut count);
                assert!(!args.is_null());
                let values: Vec<_> = std::slice::from_raw_parts(args, count as usize)
                    .iter()
                    .map(|arg| arg.to_string().unwrap())
                    .collect();
                LocalFree(Some(HLOCAL(args.cast())));
                let parsed =
                    crate::cli::Options::parse(values.into_iter().skip(1).map(Into::into)).unwrap();
                assert_eq!(parsed.profile.as_deref(), Some(id));
            }
        }
    }
}
