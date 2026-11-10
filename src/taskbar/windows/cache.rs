//! Persist resolved icon paths, validating cheap metadata before reusing them.
use super::executable::{self, FileStamp, ResolvedExecutable};
use crate::{
    config::{self, Profile},
    platform,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Task {
    pub program: String,
    pub id: String,
    pub name: String,
    pub icon: PathBuf,
    // A changed binary at the same path must refresh Windows' icon registration.
    pub stamp: Option<FileStamp>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    version: u32,
    entries: BTreeMap<PathBuf, ResolvedExecutable>,
}

#[derive(Default)]
pub(super) struct IconCache {
    entries: BTreeMap<PathBuf, ResolvedExecutable>,
    dirty: bool,
    // Keep this process-local: every new process repairs the OS list once.
    published: Option<(Option<FileStamp>, Vec<Task>)>,
}

impl IconCache {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .filter(|stored| stored.version == VERSION)
            .map(|stored| Self {
                entries: stored.entries,
                dirty: false,
                published: None,
            })
            .unwrap_or_default()
    }

    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        if self.dirty {
            let stored = Stored {
                version: VERSION,
                entries: self.entries.clone(),
            };
            config::atomic_write(path, &serde_json::to_vec(&stored)?)?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn publish_if_changed(
        &mut self,
        executable: &Path,
        tasks: &[Task],
        publish: impl FnOnce() -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let signature = (FileStamp::read(executable), tasks.to_vec());
        if self.published.as_ref() == Some(&signature) {
            return Ok(false);
        }
        publish()?;
        self.published = Some(signature);
        Ok(true)
    }

    fn resolve_with(
        &mut self,
        source: &Path,
        resolve: impl FnOnce(&Path) -> Option<ResolvedExecutable>,
    ) -> Option<ResolvedExecutable> {
        if let Some(cached) = self.entries.get(source)
            && cached.is_current_for(source)
        {
            return Some(cached.clone());
        }
        if self.entries.remove(source).is_some() {
            self.dirty = true;
        }
        let resolved = resolve(source)?;
        if !resolved
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return None;
        }
        self.entries.insert(source.to_owned(), resolved.clone());
        self.dirty = true;
        Some(resolved)
    }

    pub fn tasks(&mut self, profiles: &[Profile], fallback: &Path) -> Vec<Task> {
        // Several WSL distributions share wsl.exe. Resolve and validate each
        // program once per update, rather than once per profile.
        let mut programs = BTreeMap::new();
        let mut active = BTreeSet::new();
        let tasks = profiles
            .iter()
            .map(|profile| {
                let (icon, stamp) = programs.entry(&profile.program).or_insert_with(|| {
                    let resolved = platform::find_program(&profile.program).and_then(|source| {
                        active.insert(source.clone());
                        self.resolve_with(&source, executable::resolve_icon_source)
                    });
                    match resolved {
                        Some(resolved) => (resolved.path, resolved.dependencies.last().cloned()),
                        None => (fallback.to_owned(), FileStamp::read(fallback)),
                    }
                });
                Task {
                    program: profile.program.clone(),
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    icon: icon.clone(),
                    stamp: stamp.clone(),
                }
            })
            .collect();
        let before = self.entries.len();
        self.entries.retain(|source, _| active.contains(source));
        self.dirty |= self.entries.len() != before;
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "edolterm-icon-cache-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn exe(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, b"icon source").unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn persisted_cache_hit_skips_resolution_and_writes() {
        let fixture = Fixture::new();
        let source = fixture.exe("shell.exe");
        let path = fixture.0.join("cache.json");
        let mut cache = IconCache::default();
        cache
            .resolve_with(&source, executable::resolve_icon_source)
            .unwrap();
        cache.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut cache = IconCache::load(&path);
        let hit = cache.resolve_with(&source, |_| {
            panic!("cache hit must not resolve the executable again")
        });
        assert_eq!(hit.unwrap().path, source);
        assert!(!cache.dirty);
        cache.save(&path).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), original);
    }

    #[test]
    fn changed_alias_or_target_invalidates_the_whole_chain() {
        let fixture = Fixture::new();
        let alias = fixture.exe("alias.exe");
        let target = fixture.exe("target.exe");
        let mut cache = IconCache::default();
        let resolution = ResolvedExecutable {
            path: target.clone(),
            dependencies: vec![
                FileStamp::read(&alias).unwrap(),
                FileStamp::read(&target).unwrap(),
            ],
        };
        cache.resolve_with(&alias, |_| Some(resolution)).unwrap();
        std::fs::write(&alias, b"repointed alias").unwrap();
        let mut refreshed = false;
        cache
            .resolve_with(&alias, |_| {
                refreshed = true;
                Some(ResolvedExecutable {
                    path: target.clone(),
                    dependencies: vec![
                        FileStamp::read(&alias).unwrap(),
                        FileStamp::read(&target).unwrap(),
                    ],
                })
            })
            .unwrap();
        assert!(refreshed);
        std::fs::remove_file(&target).unwrap();
        assert!(cache.resolve_with(&alias, |_| None).is_none());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn bad_cache_recovers_and_removed_programs_are_pruned() {
        let fixture = Fixture::new();
        let path = fixture.0.join("cache.json");
        std::fs::write(&path, b"truncated {").unwrap();
        let mut cache = IconCache::load(&path);
        let source = fixture.exe("shell.exe");
        let fallback = std::env::current_exe().unwrap();
        let profiles = [
            Profile::new("a", "A", source.to_string_lossy(), &[]),
            Profile::new("b", "B", source.to_string_lossy(), &[]),
        ];
        let tasks = cache.tasks(&profiles, &fallback);
        assert_eq!(tasks[0].icon, source);
        assert_eq!(tasks[1].icon, source);
        assert_eq!(cache.entries.len(), 1);
        cache.save(&path).unwrap();
        assert!(cache.tasks(&[], &fallback).is_empty());
        cache.save(&path).unwrap();
        assert!(IconCache::load(&path).entries.is_empty());
        std::fs::write(&path, br#"{"version":999,"entries":{}}"#).unwrap();
        assert!(IconCache::load(&path).entries.is_empty());
    }

    #[test]
    fn unchanged_lists_skip_shell_calls_but_changes_and_failures_retry() {
        let fixture = Fixture::new();
        let source = fixture.exe("shell.exe");
        let fallback = std::env::current_exe().unwrap();
        let mut cache = IconCache::default();
        let mut profiles = vec![Profile::new(
            "shell",
            "Shell",
            source.to_string_lossy(),
            &[],
        )];
        let tasks = cache.tasks(&profiles, &fallback);
        assert!(
            cache
                .publish_if_changed(&fallback, &tasks, || Ok(()))
                .unwrap()
        );
        profiles[0]
            .env
            .insert("TOKEN".into(), "do-not-persist-this".into());
        let unchanged = cache.tasks(&profiles, &fallback);
        assert!(
            !cache
                .publish_if_changed(&fallback, &unchanged, || panic!(
                    "unchanged list must not call Shell"
                ))
                .unwrap()
        );
        profiles[0].name = "Renamed shell".into();
        let renamed = cache.tasks(&profiles, &fallback);
        assert!(
            cache
                .publish_if_changed(&fallback, &renamed, || Ok(()))
                .unwrap()
        );
        std::fs::write(&source, b"updated binary with a changed icon resource").unwrap();
        let updated = cache.tasks(&profiles, &fallback);
        assert_ne!(updated, renamed);
        assert!(
            cache
                .publish_if_changed(&fallback, &updated, || anyhow::bail!("Shell unavailable"))
                .is_err()
        );
        assert!(
            cache
                .publish_if_changed(&fallback, &updated, || Ok(()))
                .unwrap()
        );
        let path = fixture.0.join("cache.json");
        cache.save(&path).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("do-not-persist-this")
        );
        let mut restarted = IconCache::load(&path);
        let tasks = restarted.tasks(&profiles, &fallback);
        assert!(
            restarted
                .publish_if_changed(&fallback, &tasks, || Ok(()))
                .unwrap(),
            "new processes must repair the Windows list once"
        );
    }
}
