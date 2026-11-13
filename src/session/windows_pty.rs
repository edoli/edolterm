//! Restore level-triggered read readiness for Alacritty's Windows pipe adapter.
//!
//! In alacritty_terminal 0.26, UnblockedReader only wakes its poller when a
//! pending piper read is satisfied. EventLoop yields after MAX_LOCKED_READ
//! bytes, possibly before it reaches an empty pipe and arms that waker again.
//! Already-buffered output can then be stranded indefinitely. Faster bundled
//! ConPTY versions make this especially easy to trigger.
//!
//! Schedule a readable completion after a successful read in Level mode. This
//! guarantees another turn after a partial drain. An empty read does not post
//! anything, so there is no timer, idle polling, or idle busy loop.
use alacritty_terminal::{
    event::{OnResize, WindowSize},
    tty::{self, ChildEvent, EventedPty, EventedReadWrite},
};
use polling::{
    Event, PollMode, Poller,
    os::iocp::{CompletionPacket, PollerIocpExt},
};
use std::{
    io::{self, Read, Write},
    sync::Arc,
};

pub struct LevelTriggeredPty<T> {
    inner: T,
    poller: Option<Arc<Poller>>,
    level_readable: bool,
}

impl<T> LevelTriggeredPty<T> {
    pub fn wrap(inner: T) -> Self {
        Self {
            inner,
            poller: None,
            level_readable: false,
        }
    }

    fn interest(&mut self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        self.poller = Some(poller.clone());
        self.level_readable = event.readable && matches!(mode, PollMode::Level);
    }
}

pub fn new(
    options: &tty::Options,
    size: WindowSize,
    window_id: u64,
    title: &str,
) -> io::Result<LevelTriggeredPty<tty::Pty>> {
    tty::windows::new_with_title(options, size, window_id, title).map(LevelTriggeredPty::wrap)
}

impl<T: EventedReadWrite> Read for LevelTriggeredPty<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.reader().read(bytes)?;
        if count > 0
            && self.level_readable
            && let Some(poller) = &self.poller
        {
            // The Windows Pty maps its read/write registration to this token.
            // Do not return an error after consuming bytes if shutdown closed
            // the poller: the bytes must still be delivered to the parser.
            let _ = poller.post(CompletionPacket::new(Event::readable(
                tty::windows::PTY_READ_WRITE_TOKEN,
            )));
        }
        Ok(count)
    }
}

impl<T: EventedReadWrite> Write for LevelTriggeredPty<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.writer().write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.writer().flush()
    }
}

impl<T: EventedReadWrite> EventedReadWrite for LevelTriggeredPty<T> {
    type Reader = Self;
    type Writer = Self;

    unsafe fn register(
        &mut self,
        poller: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: The wrapper owns the underlying Pty and delegates the caller's
        // registration lifetime requirement unchanged.
        unsafe {
            self.inner.register(poller, event, mode)?;
        }
        self.interest(poller, event, mode);
        Ok(())
    }
    fn reregister(&mut self, poller: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.inner.reregister(poller, event, mode)?;
        self.interest(poller, event, mode);
        Ok(())
    }
    fn deregister(&mut self, poller: &Arc<Poller>) -> io::Result<()> {
        self.level_readable = false;
        self.poller = None;
        self.inner.deregister(poller)
    }
    fn reader(&mut self) -> &mut Self::Reader {
        self
    }
    fn writer(&mut self) -> &mut Self::Writer {
        self
    }
}

impl<T: EventedPty> EventedPty for LevelTriggeredPty<T> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}

impl<T: OnResize> OnResize for LevelTriggeredPty<T> {
    fn on_resize(&mut self, size: WindowSize) {
        self.inner.on_resize(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Cursor, Sink},
        num::NonZeroUsize,
        time::Duration,
    };

    // Model a source which signals once when a burst arrives, just like the
    // piper waker. No new producer writes occur during this regression test.
    struct Burst {
        reader: Cursor<Vec<u8>>,
        writer: Sink,
    }
    impl EventedReadWrite for Burst {
        type Reader = Cursor<Vec<u8>>;
        type Writer = Sink;
        unsafe fn register(&mut self, poll: &Arc<Poller>, _: Event, _: PollMode) -> io::Result<()> {
            poll.post(CompletionPacket::new(Event::readable(
                tty::windows::PTY_READ_WRITE_TOKEN,
            )))
        }
        fn reregister(&mut self, _: &Arc<Poller>, _: Event, _: PollMode) -> io::Result<()> {
            Ok(())
        }
        fn deregister(&mut self, _: &Arc<Poller>) -> io::Result<()> {
            Ok(())
        }
        fn reader(&mut self) -> &mut Self::Reader {
            &mut self.reader
        }
        fn writer(&mut self) -> &mut Self::Writer {
            &mut self.writer
        }
    }

    #[test]
    fn partial_drain_stays_readable_without_new_output_and_idle_stops_waking() {
        let mut pty = LevelTriggeredPty::wrap(Burst {
            reader: Cursor::new(vec![b'x'; 100_000]),
            writer: io::sink(),
        });
        let poll = Arc::new(Poller::new().unwrap());
        unsafe {
            pty.register(&poll, Event::readable(0), PollMode::Level)
                .unwrap();
        }
        let mut events = polling::Events::with_capacity(NonZeroUsize::new(8).unwrap());
        poll.wait(&mut events, Some(Duration::from_millis(100)))
            .unwrap();
        assert!(!events.is_empty());
        let mut buffer = [0; 65_536];
        assert_eq!(pty.read(&mut buffer).unwrap(), 65_536);
        events.clear();
        poll.wait(&mut events, Some(Duration::from_millis(100)))
            .unwrap();
        assert!(
            !events.is_empty(),
            "buffered data must stay readable after EventLoop yields"
        );
        assert_eq!(pty.read(&mut buffer).unwrap(), 34_464);
        events.clear();
        poll.wait(&mut events, Some(Duration::from_millis(100)))
            .unwrap();
        assert!(!events.is_empty());
        assert_eq!(pty.read(&mut buffer).unwrap(), 0);
        events.clear();
        poll.wait(&mut events, Some(Duration::from_millis(20)))
            .unwrap();
        assert!(
            events.is_empty(),
            "idle pipes must not keep waking the poller"
        );
        pty.deregister(&poll).unwrap();
    }
}
