use std::{
    io,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, ToSocketAddrs},
};

use mio::Token;

use crate::reactor::Reactor;

#[derive(Debug)]
pub struct TcpStream {
    pub(crate) stream: mio::net::TcpStream,
    pub(crate) token: Token,
}

impl TcpStream {
    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let stream = std::net::TcpStream::connect(addr)?;
        Reactor::get().register_tcp_stream(stream)
    }

    pub async fn write(&self, buf: &[u8]) -> io::Result<usize> {
        would_block_future(|| (&self.stream).write(buf), self.token).await
    }

    pub async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        would_block_future(|| (&self.stream).read(buf), self.token).await
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        Reactor::get().deregister_tcp_stream(self);
    }
}

#[derive(Debug)]
pub struct TcpListener {
    pub(crate) listener: mio::net::TcpListener,
    pub(crate) token: Token,
}

impl TcpListener {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let stream = std::net::TcpListener::bind(addr)?;
        Reactor::get().register_tcp_listener(stream)
    }

    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (stream, addr) = would_block_future(|| self.listener.accept(), self.token).await?;
        let stream = Reactor::get().register_mio_tcp_stream(stream)?;

        Ok((stream, addr))
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        Reactor::get().deregister_tcp_listener(self);
    }
}

#[derive(Debug)]
pub struct UdpSocket {
    pub(crate) socket: mio::net::UdpSocket,
    pub(crate) token: Token,
}

impl UdpSocket {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind(addr)?;
        Reactor::get().register_udp_socket(socket)
    }

    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        would_block_future(|| self.socket.send(buf), self.token).await
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        would_block_future(|| self.socket.recv(buf), self.token).await
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        Reactor::get().deregister_udp_socket(self);
    }
}

/// Handles `io::Error`s with `ErrorKind::WouldBlock` returned by the given
/// closure. This is done by waiting until it would no longer block, using the
/// token associated to the task to poll the reactor.
async fn would_block_future<T>(
    mut f: impl FnMut() -> io::Result<T>,
    token: Token,
) -> io::Result<T> {
    loop {
        match f() {
            Ok(value) => return Ok(value),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::future::poll_fn(|cx| Reactor::get().poll(token, cx)).await?
            }
            Err(error) => return Err(error),
        }
    }
}
