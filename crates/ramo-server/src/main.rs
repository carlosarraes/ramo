#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt().compact().try_init();
    if let Err(failure) = ramo_server::run().await {
        eprintln!("{failure}");
        std::process::exit(1);
    }
}
