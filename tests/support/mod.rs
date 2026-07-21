use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ramo::session::SessionClient;

pub struct TestSessionDaemon {
    client: SessionClient,
    child: Child,
}

impl TestSessionDaemon {
    pub fn spawn() -> Self {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = SessionClient::new(reservation.local_addr().unwrap());
        drop(reservation);

        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("ramo"))
            .args(["daemon", "serve"])
            .env("RAMO_SESSION_HOST", client.address().ip().to_string())
            .env("RAMO_SESSION_PORT", client.address().port().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while client.capabilities().is_err() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("test session broker exited before startup: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "test session broker did not start at {}",
                client.address()
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        Self { client, child }
    }

    pub const fn client(&self) -> SessionClient {
        self.client
    }
}

impl Drop for TestSessionDaemon {
    fn drop(&mut self) {
        let _ = self.client.shutdown();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
