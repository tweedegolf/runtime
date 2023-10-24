use std::time::Instant;

use runtime::net::TcpStream;

#[runtime::main]
async fn main() {
    let stream = TcpStream::connect("127.0.0.1:8000").expect("failed to connect");

    println!("connected, waiting for start signal...");

    let mut buf = [0; 1024];
    loop {
        let length = stream.read(&mut buf).await.unwrap();
        if &buf[..length] == b"GO" {
            println!("GO");
            break;
        }
    }

    let start = Instant::now();
    let mut last = 0;
    loop {
        while start.elapsed().as_secs() <= last {
            std::hint::spin_loop();
        }

        last += 1;
        let message = last.to_string();
        let bytes = message.as_bytes();

        stream
            .write(bytes)
            .await
            .expect("failed to write to stream");

        println!("sent {message}")
    }
}
