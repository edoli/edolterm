//! Resolve icon sources without changing how packaged applications are launched.
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
    },
    System::{
        IO::DeviceIoControl, Ioctl::FSCTL_GET_REPARSE_POINT,
        SystemServices::IO_REPARSE_TAG_APPEXECLINK,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct FileStamp {
    path: PathBuf,
    attributes: u32,
    size: u64,
    created: u64,
    modified: u64,
}

impl FileStamp {
    pub(super) fn read(path: &Path) -> Option<Self> {
        Some(Self::from_metadata(path, &fs::symlink_metadata(path).ok()?))
    }

    fn from_metadata(path: &Path, metadata: &fs::Metadata) -> Self {
        Self {
            path: path.to_owned(),
            attributes: metadata.file_attributes(),
            size: metadata.file_size(),
            created: metadata.creation_time(),
            modified: metadata.last_write_time(),
        }
    }

    fn is_current(&self) -> bool {
        Self::read(&self.path).as_ref() == Some(self)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ResolvedExecutable {
    pub path: PathBuf,
    pub dependencies: Vec<FileStamp>,
}

impl ResolvedExecutable {
    pub fn is_current_for(&self, source: &Path) -> bool {
        self.dependencies
            .first()
            .is_some_and(|stamp| stamp.path == source)
            && self
                .dependencies
                .last()
                .is_some_and(|stamp| stamp.path == self.path && stamp.size > 0)
            && self.dependencies.iter().all(FileStamp::is_current)
    }
}

#[cfg(test)]
pub(super) fn resolve_icon_executable(path: &Path) -> Option<PathBuf> {
    resolve_icon_source(path).map(|resolved| resolved.path)
}

pub(super) fn resolve_icon_source(path: &Path) -> Option<ResolvedExecutable> {
    let mut path = path.to_owned();
    let mut dependencies = Vec::new();
    // Both ordinary links and app execution aliases can point to another link.
    // Bound the traversal so broken or cyclic links cannot stall list updates.
    for _ in 0..8 {
        let metadata = fs::symlink_metadata(&path).ok()?;
        dependencies.push(FileStamp::from_metadata(&path, &metadata));
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            if let Ok(target) = fs::read_link(&path) {
                path = if target.is_absolute() {
                    target
                } else {
                    path.parent()?.join(target)
                };
                continue;
            }
            if let Some(target) = read_app_execution_alias(&path) {
                path = target;
                continue;
            }
        }
        // An unresolved Store alias is a zero-byte placeholder, not an icon
        // resource. Let the caller use its default icon in that case.
        return (metadata.is_file() && metadata.len() > 0)
            .then_some(ResolvedExecutable { path, dependencies });
    }
    None
}

fn read_app_execution_alias(path: &Path) -> Option<PathBuf> {
    // Open the reparse point itself with no data access. This does not launch
    // the app, and does not require access to the protected package directory.
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let mut buffer = [0u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
    let mut returned = 0;
    // SAFETY: file owns a valid synchronous handle, buffer is writable for its
    // advertised length, and returned is a valid output pointer.
    let success = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return None;
    }
    app_execution_alias_target(buffer.get(..returned as usize)?)
}

fn app_execution_alias_target(data: &[u8]) -> Option<PathBuf> {
    // REPARSE_DATA_BUFFER header, followed by AppExecLink version 3 and a
    // UTF-16 multistring: package family, app ID, executable, app type.
    // Other tags/versions are deliberately left to the normal icon fallback.
    // Layout also used by CPython's AppExecLinkFile:
    // https://github.com/python/cpython/blob/3.13/PC/launcher2.c
    let tag = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
    if tag != IO_REPARSE_TAG_APPEXECLINK {
        return None;
    }
    let length = u16::from_le_bytes(data.get(4..6)?.try_into().ok()?) as usize;
    let payload = data.get(8..8 + length)?;
    let version = u32::from_le_bytes(payload.get(..4)?.try_into().ok()?);
    if version != 3 || length % 2 != 0 {
        return None;
    }
    let strings: Vec<u16> = payload
        .get(4..)?
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let mut strings = strings.split_inclusive(|unit| *unit == 0);
    for _ in 0..2 {
        let field = strings.next()?;
        if field.last() != Some(&0) {
            return None;
        }
    }
    let executable = strings.next()?;
    if executable.last() != Some(&0) {
        return None;
    }
    let target = PathBuf::from(OsString::from_wide(&executable[..executable.len() - 1]));
    target.is_absolute().then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reparse_data(target: &str) -> Vec<u8> {
        let strings = format!("Example_123\0Example_123!App\0{target}\0Desktop\0");
        let mut payload = 3u32.to_le_bytes().to_vec();
        payload.extend(strings.encode_utf16().flat_map(u16::to_le_bytes));
        let mut data = IO_REPARSE_TAG_APPEXECLINK.to_le_bytes().to_vec();
        data.extend((payload.len() as u16).to_le_bytes());
        data.extend([0u8; 2]);
        data.extend(payload);
        data
    }

    #[test]
    fn app_execution_alias_decodes_unicode_target_and_rejects_invalid_records() {
        let target = r"C:\Program Files\WindowsApps\개발 셸_1.0\pwsh.exe";
        let data = reparse_data(target);
        assert_eq!(
            app_execution_alias_target(&data),
            Some(PathBuf::from(target))
        );
        for length in 0..data.len() {
            assert!(app_execution_alias_target(&data[..length]).is_none());
        }
        let mut wrong_tag = data.clone();
        wrong_tag[..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(app_execution_alias_target(&wrong_tag).is_none());
        let mut wrong_version = data.clone();
        wrong_version[8..12].copy_from_slice(&99u32.to_le_bytes());
        assert!(app_execution_alias_target(&wrong_version).is_none());
        assert!(app_execution_alias_target(&reparse_data("relative.exe")).is_none());
        assert!(app_execution_alias_target(&reparse_data("")).is_none());
        let mut unterminated = reparse_data(target);
        unterminated.truncate(unterminated.len() - "Desktop\0".encode_utf16().count() * 2 - 2);
        let length = (unterminated.len() - 8) as u16;
        unterminated[4..6].copy_from_slice(&length.to_le_bytes());
        assert!(app_execution_alias_target(&unterminated).is_none());
    }

    #[test]
    fn normal_executables_are_preserved_and_missing_files_fall_back() {
        let executable = std::env::current_exe().unwrap();
        assert_eq!(
            resolve_icon_executable(&executable),
            Some(executable.clone())
        );
        assert!(resolve_icon_executable(&executable.with_extension("missing-file")).is_none());
    }
}
