mod server;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:7878").await?;
    println!("Server listening on http://127.0.0.1:7878");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = server::handle_connection(stream).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}
