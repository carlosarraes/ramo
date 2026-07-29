#[tokio::main]
async fn main() {
    if let Err(failure) = ramo_server::run().await {
        eprintln!("{failure}");
        std::process::exit(1);
    }
}
