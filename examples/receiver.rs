use runtime::net::TcpListener;

#[runtime::main]
async fn main() {
    let stream = TcpListener::bind("127.0.0.1:8000").expect("failed to bind");

    println!("waiting for connections...");

    while let Ok((stream, addr)) = stream.accept().await {
        println!("connected to {addr}");

        // Instruct sender to start sending messages.
        stream.write(b"GO").await.unwrap();

        let mut buf = [0; 1024];

        while let Ok(length) = stream.read(&mut buf).await {
            let message = String::from_utf8_lossy(&buf[..length]);
            println!("[{addr}] {message}");
        }
    }
}
