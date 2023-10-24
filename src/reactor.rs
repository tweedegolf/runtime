use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
    task::{Context, Poll, Waker},
    thread,
};

use mio::{Events, Interest, Registry, Token};

#[derive(Debug)]
pub struct Reactor {
    registry: Registry,
    statuses: Mutex<HashMap<Token, Status>>,
}

static CURRENT_TOKEN: AtomicUsize = AtomicUsize::new(0);

impl Reactor {
    pub fn get() -> &'static Self {
        static REACTOR: OnceLock<Reactor> = OnceLock::new();

        REACTOR.get_or_init(|| {
            let poll = mio::Poll::new().expect("failed to create poll");
            let reactor = Reactor {
                registry: poll
                    .registry()
                    .try_clone()
                    .expect("failed to clone registry"),
                statuses: Mutex::new(HashMap::new()),
            };

            thread::Builder::new()
                .name("reactor".to_owned())
                .spawn(|| run(poll))
                .expect("failed to spawn reactor thread");

            reactor
        })
    }

    pub fn register_mio_tcp_stream(
        &self,
        mut stream: mio::net::TcpStream,
    ) -> io::Result<crate::net::TcpStream> {
        let token = Token(CURRENT_TOKEN.fetch_add(1, Ordering::Relaxed));

        self.registry
            .register(&mut stream, token, Interest::READABLE | Interest::WRITABLE)?;

        Ok(crate::net::TcpStream { stream, token })
    }

    pub fn register_tcp_stream(
        &self,
        stream: std::net::TcpStream,
    ) -> io::Result<crate::net::TcpStream> {
        stream.set_nonblocking(true)?;
        let stream = mio::net::TcpStream::from_std(stream);
        self.register_mio_tcp_stream(stream)
    }

    pub fn deregister_tcp_stream(&self, stream: &mut crate::net::TcpStream) {
        self.registry.deregister(&mut stream.stream).unwrap();
    }

    pub fn register_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<crate::net::TcpListener> {
        listener.set_nonblocking(true)?;
        let mut listener = mio::net::TcpListener::from_std(listener);
        let token = Token(CURRENT_TOKEN.fetch_add(1, Ordering::Relaxed));

        self.registry.register(
            &mut listener,
            token,
            Interest::READABLE | Interest::WRITABLE,
        )?;

        Ok(crate::net::TcpListener { listener, token })
    }

    pub fn deregister_tcp_listener(&self, listener: &mut crate::net::TcpListener) {
        self.registry.deregister(&mut listener.listener).unwrap();
    }

    pub fn register_udp_socket(
        &self,
        socket: std::net::UdpSocket,
    ) -> io::Result<crate::net::UdpSocket> {
        socket.set_nonblocking(true)?;
        let mut socket = mio::net::UdpSocket::from_std(socket);
        let token = Token(CURRENT_TOKEN.fetch_add(1, Ordering::Relaxed));

        self.registry
            .register(&mut socket, token, Interest::READABLE | Interest::WRITABLE)?;

        Ok(crate::net::UdpSocket { socket, token })
    }

    pub fn deregister_udp_socket(&self, socket: &mut crate::net::UdpSocket) {
        self.registry.deregister(&mut socket.socket).unwrap();
    }

    pub fn poll(&self, token: Token, cx: &mut Context) -> Poll<io::Result<()>> {
        let mut guard = self.statuses.lock().unwrap();
        match guard.get(&token) {
            None => {
                // The awaited event has not happened yet and has not been polled before. Store
                // the waker so that the task may continue as soon as the event gets processed
                // by `run`.
                guard.insert(token, Status::Awaited(cx.waker().clone()));
                Poll::Pending
            }
            Some(Status::Awaited(waker)) => {
                // The event was polled before, but has not happened in the meantime. Update the
                // waker if the task has moved.
                if !waker.will_wake(cx.waker()) {
                    guard.insert(token, Status::Awaited(cx.waker().clone()));
                }
                Poll::Pending
            }
            Some(Status::Happened) => {
                // Event happened before this poll. Continue immediately.
                guard.remove(&token);
                Poll::Ready(Ok(()))
            }
        }
    }
}

fn run(mut poll: mio::Poll) -> ! {
    let reactor = Reactor::get();
    let mut events = Events::with_capacity(1024);

    loop {
        poll.poll(&mut events, None)
            .expect("failed to poll for events");

        for event in &events {
            let mut guard = reactor.statuses.lock().unwrap();

            // Indicate that the event has happened, instructing `poll` to return
            // immediately.
            let previous = guard.insert(event.token(), Status::Happened);

            // If the event was being awaited, wake the waiting task so that it may
            // continue.
            if let Some(Status::Awaited(waker)) = previous {
                waker.wake();
            }
        }
    }
}

/// Represents the status of an event.
///
/// For an `Option<Status>`:
///
/// 1. `None` indicates that the event wasn't polled yet and hasn't happened yet
///    either.
/// 2. `Some(Status::Awaited(waker))` indicates that the event was polled, but
///    hasn't happened yet. `waker.wake()` should be called when the event
///    happens.
/// 3. `Some(Status::Happened)` indicates that the event happened. If it was
///    being awaited before it happened, the awaiting task was woken up.
///
/// Whenever a task awaits an event with `Status::Happened`, it can continue
/// immediately.
#[derive(Debug)]
pub enum Status {
    /// The event is currently being awaited.
    Awaited(Waker),
    /// The event happened before it was (re-)awaited, remember this so the
    /// awaiting task can continue immediately.
    Happened,
}
