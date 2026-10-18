use super::*;
use std::{io, net::Shutdown, thread, time::Instant};

pub(super) fn run(address: SocketAddr, authorization: &str) {
    let upstream: SocketAddr = fs::read_to_string("context-proxy-endpoint")
        .unwrap()
        .parse()
        .unwrap();
    assert!(upstream.ip().is_loopback());
    fs::write("context-proxy-authorization", authorization).unwrap();
    let listener = TcpListener::bind(address).unwrap();
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut connections: Vec<thread::JoinHandle<()>> = Vec::new();
    while connections.len() < 2 && connections.first().is_none_or(|first| !first.is_finished()) {
        assert!(
            Instant::now() < deadline,
            "context transport lifetime exceeded"
        );
        match listener.accept() {
            Ok((incoming, _)) => {
                connections.push(thread::spawn(move || forward(incoming, upstream)));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("context transport accept failed: {error}"),
        }
    }
    for connection in connections {
        connection.join().unwrap();
    }
}

fn forward(mut incoming: TcpStream, upstream: SocketAddr) {
    let mut outgoing = TcpStream::connect_timeout(&upstream, Duration::from_secs(10)).unwrap();
    for stream in [&incoming, &outgoing] {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
    }
    let mut incoming_return = incoming.try_clone().unwrap();
    let mut outgoing_return = outgoing.try_clone().unwrap();
    let send = thread::spawn(move || {
        let result = io::copy(&mut incoming, &mut outgoing);
        let _ = incoming.shutdown(Shutdown::Both);
        let _ = outgoing.shutdown(Shutdown::Both);
        result
    });
    let result = io::copy(&mut outgoing_return, &mut incoming_return);
    let _ = incoming_return.shutdown(Shutdown::Both);
    let _ = outgoing_return.shutdown(Shutdown::Both);
    allow_closed(result);
    allow_closed(send.join().unwrap());
}

fn allow_closed(result: io::Result<u64>) {
    if let Err(error) = result {
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::NotConnected
                    | io::ErrorKind::UnexpectedEof
            ),
            "context transport failed: {error}",
        );
    }
}
