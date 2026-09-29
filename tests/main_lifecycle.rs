//! Process-level startup and graceful-shutdown contract.
//!
//! The child uses only reserved example credentials and loopback ports. The
//! UniFi endpoint is intentionally closed so the first poll exercises the
//! unavailable path without contacting a controller.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn binary_polls_a_local_endpoint_and_shuts_down_cleanly_on_sigint() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    let unifi_port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_unifi-apclients-mqtt"))
            .env_clear()
            .env("UNIFI_URL", format!("https://127.0.0.1:{unifi_port}"))
            .env("UNIFI_USERNAME", "fixture-user")
            .env("UNIFI_PASSWORD", "fixture-secret")
            .env("UNIFI_AP_MACS", "11:22:33:44:55:66")
            .env("UNIFI_POLL_INTERVAL_SECS", "3600")
            .env("UNIFI_TLS_INSECURE", "true")
            .env("MQTT_HOST", "127.0.0.1")
            .env("MQTT_PORT", "1")
            .stdout(Stdio::piped())
            .spawn()
            .expect("service binary should start"),
    );
    let stdout = child.0.stdout.take().expect("stdout should be piped");
    let (log_sender, log_receiver) = mpsc::channel();
    let log_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = log_sender.send(line);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut log_lines = Vec::new();
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match log_receiver.recv_timeout(remaining) {
            Ok(line) => {
                started |= line.contains("UniFi AP client poller started");
                log_lines.push(line);
            }
            Err(error) => panic!("service did not finish startup: {error}; logs: {log_lines:?}"),
        }
    }
    assert!(started, "service should finish startup");
    thread::sleep(Duration::from_millis(100));

    let signal = Command::new("kill")
        .args(["-INT", &child.0.id().to_string()])
        .status()
        .expect("send SIGINT to child");
    assert!(signal.success(), "SIGINT should be delivered");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("wait for service process") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "service did not shut down after SIGINT"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "service should exit successfully: {status}"
    );

    while let Ok(line) = log_receiver.try_recv() {
        log_lines.push(line);
    }
    log_reader.join().expect("stdout reader should finish");
    assert!(
        log_lines
            .iter()
            .any(|line| line.contains("UniFi poll failed")),
        "first local UniFi poll should fail: {log_lines:?}"
    );
    assert!(
        log_lines
            .iter()
            .any(|line| line.contains("shutting down UniFi AP client poller")),
        "shutdown should be logged after SIGINT: {log_lines:?}"
    );
}
