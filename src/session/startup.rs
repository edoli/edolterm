//! Serialize expensive PTY creation off the UI thread, with selected tabs first.
use super::*;
use std::{collections::VecDeque, sync::atomic::AtomicU64};

pub type Started = Result<(Session, async_channel::Receiver<Event>)>;

#[derive(Clone)]
pub struct Launcher {
    commands: async_channel::Sender<Command>,
    next_id: Arc<AtomicU64>,
}

enum Command {
    Start(Job, bool),
    Prioritize(u64),
}

struct Job {
    id: u64,
    cancelled: Arc<AtomicBool>,
    work: Box<dyn FnOnce() -> Started + Send>,
    result: async_channel::Sender<Started>,
    completed: async_channel::Sender<()>,
}

pub struct Pending {
    id: u64,
    commands: async_channel::Sender<Command>,
    cancelled: Arc<AtomicBool>,
    pub result: async_channel::Receiver<Started>,
    completed: async_channel::Receiver<()>,
}

impl Launcher {
    pub fn new() -> std::io::Result<Self> {
        let (commands, receiver) = async_channel::unbounded();
        std::thread::Builder::new()
            .name("edolterm-session-start".into())
            .spawn(move || worker(receiver))?;
        Ok(Self {
            commands,
            next_id: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn start(
        &self,
        profile: Profile,
        cwd: Option<PathBuf>,
        settings: Settings,
        window_id: u64,
        selected: bool,
    ) -> Pending {
        self.submit(
            Box::new(move || Session::start(profile, cwd, &settings, window_id)),
            selected,
        )
    }

    fn submit(&self, work: Box<dyn FnOnce() -> Started + Send>, selected: bool) -> Pending {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (result, receiver) = async_channel::bounded(1);
        let (completed, completion) = async_channel::bounded(1);
        let _ = self.commands.try_send(Command::Start(
            Job {
                id,
                cancelled: cancelled.clone(),
                work,
                result,
                completed,
            },
            selected,
        ));
        Pending {
            id,
            commands: self.commands.clone(),
            cancelled,
            result: receiver,
            completed: completion,
        }
    }
}

impl Pending {
    #[cfg(all(feature = "ui-tests", debug_assertions))]
    pub(crate) fn sequence(&self) -> u64 {
        self.id
    }

    pub fn prioritize(&self) {
        let _ = self.commands.try_send(Command::Prioritize(self.id));
    }

    pub fn cancel(&self) -> Vec<async_channel::Receiver<()>> {
        self.cancelled.store(true, Ordering::Release);
        // Close all receiver clones before draining a result that raced cancellation.
        self.result.close();
        let mut completed = vec![self.completed.clone()];
        while let Ok(result) = self.result.try_recv() {
            if let Ok((session, _)) = result {
                session.shutdown();
                completed.push(session.completed.clone());
            }
        }
        completed
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn enqueue(command: Command, jobs: &mut VecDeque<Job>) {
    match command {
        Command::Start(job, true) => jobs.push_front(job),
        Command::Start(job, false) => jobs.push_back(job),
        Command::Prioritize(id) => {
            if let Some(index) = jobs.iter().position(|job| job.id == id) {
                let job = jobs.remove(index).unwrap();
                jobs.push_front(job);
            }
        }
    }
}

fn worker(commands: async_channel::Receiver<Command>) {
    let mut jobs = VecDeque::new();
    loop {
        if jobs.is_empty() {
            let Ok(command) = commands.recv_blocking() else {
                return;
            };
            enqueue(command, &mut jobs);
        }
        while let Ok(command) = commands.try_recv() {
            enqueue(command, &mut jobs);
        }
        let Some(job) = jobs.pop_front() else {
            continue;
        };
        if !job.cancelled.load(Ordering::Acquire) {
            let result = (job.work)();
            if let Err(error) = job.result.try_send(result)
                && let Ok((session, _)) = error.into_inner()
            {
                // A closed tab must not leave a newly created child running. This
                // wait is on the worker, never on the foreground event loop.
                session.shutdown();
                let _ = session.completed.recv_blocking();
            }
        }
        let _ = job.completed.try_send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn selection_promotes_a_waiting_tab_and_cancelled_tabs_never_start() {
        let launcher = Launcher::new().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first = launcher.submit(
            Box::new(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                anyhow::bail!("first")
            }),
            false,
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (order_tx, order_rx) = mpsc::channel();
        let request = |id, selected| {
            let order_tx = order_tx.clone();
            launcher.submit(
                Box::new(move || {
                    order_tx.send(id).unwrap();
                    anyhow::bail!("test result")
                }),
                selected,
            )
        };
        let normal = request(1, false);
        let selected = request(2, true);
        let promoted = request(3, false);
        let cancelled = request(4, true);
        promoted.prioritize();
        drop(cancelled);
        release_tx.send(()).unwrap();
        assert_eq!(order_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 3);
        assert_eq!(order_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
        assert_eq!(order_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        normal.completed.recv_blocking().unwrap();
        assert!(order_rx.try_recv().is_err());
        drop((first, normal, selected, promoted));
    }

    #[test]
    #[cfg(windows)]
    fn cancelling_inflight_and_unclaimed_starts_waits_for_child_cleanup() {
        for unclaimed in [false, true] {
            let launcher = Launcher::new().unwrap();
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let (child_tx, child_rx) = mpsc::channel();
            let pending = launcher.submit(
                Box::new(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    let result = Session::start(
                        Profile::new("cmd", "CMD", "cmd.exe", &["/D"]),
                        None,
                        &Settings::default(),
                        0,
                    );
                    if let Ok((session, _)) = &result {
                        child_tx.send(session.completed.clone()).unwrap();
                    }
                    result
                }),
                true,
            );
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let completion = if unclaimed {
                release_tx.send(()).unwrap();
                pending.completed.recv_blocking().unwrap();
                assert!(!pending.result.is_empty());
                pending.cancel()
            } else {
                let completion = pending.cancel();
                release_tx.send(()).unwrap();
                completion
            };
            let child = child_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while completion
                .iter()
                .chain([&child])
                .any(|r| r.is_empty() && !r.is_closed())
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "cancelled child survived"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
