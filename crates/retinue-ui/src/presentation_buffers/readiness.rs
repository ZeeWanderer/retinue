use rustix::event::{PollFd, PollFlags, poll};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    thread::{self, JoinHandle},
};
use wayland_client::backend::Backend;

const IDLE: u8 = 0;
const WAITING: u8 = 1;
const STOP: u8 = 2;

/// An on-demand reader for a foreign display's separate native event queue.
pub struct QueueWake {
    signal: UnixStream,
    state: Arc<AtomicU8>,
    worker: Option<JoinHandle<()>>,
}

impl QueueWake {
    pub fn new(backend: Backend, wake: impl Fn() + Send + 'static) -> io::Result<Self> {
        let (signal, input) = UnixStream::pair()?;
        signal.set_nonblocking(true)?;
        input.set_nonblocking(true)?;
        let state = Arc::new(AtomicU8::new(IDLE));
        let worker_state = state.clone();
        let worker = thread::Builder::new()
            .name("wayland-ready".into())
            .spawn(move || {
                if run(&backend, input, &worker_state, &wake).is_err()
                    && worker_state.load(Ordering::Acquire) != STOP
                {
                    worker_state.store(IDLE, Ordering::Release);
                    wake();
                }
            })?;
        Ok(Self {
            signal,
            state,
            worker: Some(worker),
        })
    }

    pub fn waiting(&self) -> bool {
        self.state.load(Ordering::Acquire) == WAITING
    }

    pub fn arm(&self) -> io::Result<()> {
        if self.state.swap(WAITING, Ordering::AcqRel) != WAITING {
            self.signal()?;
        }
        Ok(())
    }

    pub fn cancel(&self) {
        if self.state.swap(IDLE, Ordering::AcqRel) == WAITING {
            let _ = self.signal();
        }
    }

    fn signal(&self) -> io::Result<()> {
        match (&self.signal).write(&[1]) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Drop for QueueWake {
    fn drop(&mut self) {
        self.state.store(STOP, Ordering::Release);
        let _ = self.signal();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(
    backend: &Backend,
    mut input: UnixStream,
    state: &AtomicU8,
    wake: &impl Fn(),
) -> Result<(), String> {
    loop {
        match state.load(Ordering::Acquire) {
            STOP => return Ok(()),
            IDLE => {
                wait(&mut [PollFd::new(&input, PollFlags::IN)])?;
                drain(&mut input)?;
            }
            _ => {
                if backend.dispatch_inner_queue().map_err(|e| e.to_string())? > 0 {
                    notify(state, wake);
                    continue;
                }
                let Some(read) = backend.prepare_read() else {
                    continue;
                };
                let mut fds = [
                    PollFd::from_borrowed_fd(read.connection_fd(), PollFlags::IN),
                    PollFd::new(&input, PollFlags::IN),
                ];
                wait(&mut fds)?;
                if !fds[1].revents().is_empty() {
                    drop(read);
                    drain(&mut input)?;
                    continue;
                }
                match read.read() {
                    Ok(count) if count > 0 => notify(state, wake),
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(e))
                        if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }
}

fn notify(state: &AtomicU8, wake: &impl Fn()) {
    if state
        .compare_exchange(WAITING, IDLE, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        wake();
    }
}

fn wait(fds: &mut [PollFd<'_>]) -> Result<(), String> {
    loop {
        match poll(fds, None) {
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn drain(input: &mut UnixStream) -> Result<(), String> {
    loop {
        match input.read(&mut [0; 64]) {
            Ok(0) => return Err("presentation wake pipe closed".into()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}
