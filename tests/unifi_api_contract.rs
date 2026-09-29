//! UniFi HTTP client contract tests.
//!
//! The fixture is deliberately a local `std::net::TcpListener`: it checks the
//! wire requests and session cookie without mocking the HTTP client.
//!
//! Expected production interface:
//! `unifi::UniFiClient::new(&Config)` creates a client and
//! `fetch_snapshots(&[String])` logs in, looks up AP names and returns the
//! mapped snapshots plus AP MACs that UniFi did not return.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use unifi_apclients_mqtt::config::Config;
use unifi_apclients_mqtt::unifi::UniFiClient;

const AP_ONE: &str = "11:22:33:44:55:66";
const AP_TWO: &str = "22:33:44:55:66:77";

fn config_for(base_url: &str) -> Config {
    Config {
        unifi_url: base_url.to_owned(),
        unifi_username: "collector".to_owned(),
        unifi_password: "secret".to_owned(),
        ap_macs: vec![AP_ONE.to_owned(), AP_TWO.to_owned()],
        poll_interval: Duration::from_secs(5),
        unifi_tls_insecure: false,
        mqtt_host: "mqtt.example.test".to_owned(),
        mqtt_port: 1883,
        mqtt_username: None,
        mqtt_password: None,
        mqtt_base_topic: "unifi/apclients".to_owned(),
        homeassistant_discovery_enabled: true,
        homeassistant_discovery_prefix: "homeassistant".to_owned(),
        homeassistant_status_topic: "homeassistant/status".to_owned(),
    }
}

struct Fixture {
    base_url: String,
    requests: Receiver<Result<String, String>>,
    thread: JoinHandle<()>,
}

fn start_fixture(fail_client_stats: bool) -> Fixture {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local fixture");
    let address = listener.local_addr().expect("fixture address");
    let (sender, requests) = mpsc::channel();

    let thread = thread::spawn(move || {
        let mut cookie_seen = false;
        for request_number in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            match read_request(&mut stream) {
                Ok((path, headers, body)) => {
                    let result =
                        handle_request(request_number, &path, &headers, &body, &mut cookie_seen);
                    let _ = sender.send(result.clone().map(|_| path.to_owned()));
                    match result {
                        Ok(()) => {
                            let body = match request_number {
                                0 => r#"{"meta":{"rc":"ok"},"data":[]}"#,
                                1 => {
                                    r#"{"meta":{"rc":"ok"},"data":[{"mac":"11:22:33:44:55:66","name":"Living Room AP"},{"mac":"22:33:44:55:66:77","name":"Office AP"},{"mac":"33:44:55:66:77:88","name_override":"Guest AP"},{"name":"device without a MAC"}]}"#
                                }
                                2 if fail_client_stats => r#"{"meta":{"rc":"error"},"data":[]}"#,
                                2 => {
                                    r#"{"meta":{"rc":"ok"},"data":[{"mac":"AA:AA:AA:AA:AA:01","name":"living-room-phone","ap_mac":"11:22:33:44:55:66","ip":"192.0.2.21"},{"mac":"AA:AA:AA:AA:AA:02","name":"office-laptop","ap_mac":"22:33:44:55:66:77","ip":"192.0.2.22"}]}"#
                                }
                                _ => unreachable!(),
                            };
                            write_response(&mut stream, 200, body);
                        }
                        Err(error) => {
                            write_response(&mut stream, 400, &format!("{{\"error\":\"{error}\"}}"));
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(Err(error));
                }
            }
        }
    });

    Fixture {
        base_url: format!("http://{address}"),
        requests,
        thread,
    }
}

fn start_status_fixture(login_status: u16, device_status: u16) -> Fixture {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local fixture");
    let address = listener.local_addr().expect("fixture address");
    let (sender, requests) = mpsc::channel();

    let thread = thread::spawn(move || {
        let mut cookie_seen = false;
        for (request_number, status) in [(0, login_status), (1, device_status)] {
            if request_number == 1 && login_status != 200 {
                break;
            }
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let (path, headers, body) = read_request(&mut stream).expect("read fixture request");
            handle_request(request_number, &path, &headers, &body, &mut cookie_seen)
                .expect("request should follow the expected API contract");
            sender.send(Ok(path)).expect("test should receive request");
            write_response(&mut stream, status, r#"{"meta":{"rc":"ok"},"data":[]}"#);
        }
    });

    Fixture {
        base_url: format!("http://{address}"),
        requests,
        thread,
    }
}

fn start_expired_session_fixture() -> Fixture {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local fixture");
    let address = listener.local_addr().expect("fixture address");
    let (sender, requests) = mpsc::channel();

    let thread = thread::spawn(move || {
        let mut cookie_seen = false;
        for request_number in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let (path, headers, body) = read_request(&mut stream).expect("read fixture request");
            let contract_number = match request_number {
                0 | 2 => 0,
                1 | 3 => 1,
                4 => 2,
                _ => unreachable!(),
            };
            handle_request(contract_number, &path, &headers, &body, &mut cookie_seen)
                .expect("request should follow the expected API contract");
            sender.send(Ok(path)).expect("test should receive request");
            let (status, response) = match request_number {
                0 | 2 => (200, r#"{"meta":{"rc":"ok"},"data":[]}"#),
                1 => (401, r#"{"meta":{"rc":"error"},"data":[]}"#),
                3 => (
                    200,
                    r#"{"meta":{"rc":"ok"},"data":[{"mac":"11:22:33:44:55:66","name":"Living Room AP"}]}"#,
                ),
                4 => (200, r#"{"meta":{"rc":"ok"},"data":[]}"#),
                _ => unreachable!(),
            };
            write_response(&mut stream, status, response);
        }
    });

    Fixture {
        base_url: format!("http://{address}"),
        requests,
        thread,
    }
}

fn start_optional_metadata_fixture() -> Fixture {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local fixture");
    let address = listener.local_addr().expect("fixture address");
    let (sender, requests) = mpsc::channel();

    let thread = thread::spawn(move || {
        let mut cookie_seen = false;
        for request_number in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let (path, headers, body) = read_request(&mut stream).expect("read fixture request");
            handle_request(request_number, &path, &headers, &body, &mut cookie_seen)
                .expect("request should follow the expected API contract");
            sender.send(Ok(path)).expect("test should receive request");
            let response = match request_number {
                0 | 2 => r#"{"data":[]}"#,
                1 => r#"{"meta":{},"data":[{"mac":"11:22:33:44:55:66","name":"Fixture AP"}]}"#,
                _ => unreachable!(),
            };
            write_response(&mut stream, 200, response);
        }
    });

    Fixture {
        base_url: format!("http://{address}"),
        requests,
        thread,
    }
}

fn read_request(stream: &mut TcpStream) -> Result<(String, String, String), String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end;
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("client closed request before headers".to_owned());
        }
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = end + 4;
            break;
        }
    }

    let header_text = String::from_utf8(bytes[..header_end].to_vec()).map_err(|e| e.to_string())?;
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then_some(value.trim())
        })
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("client closed request before body".to_owned());
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_owned())?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "missing request path".to_owned())?
        .to_owned();
    let body = String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
        .map_err(|e| e.to_string())?;
    Ok((path, header_text, body))
}

fn handle_request(
    request_number: usize,
    path: &str,
    headers: &str,
    body: &str,
    cookie_seen: &mut bool,
) -> Result<(), String> {
    match request_number {
        0 => {
            if path != "/api/login" {
                return Err(format!("expected login path, got {path}"));
            }
            let body: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
            if body["username"] != "collector" || body["password"] != "secret" {
                return Err(format!("unexpected login body: {body}"));
            }
            *cookie_seen = true;
            Ok(())
        }
        1 => {
            require_cookie(headers, cookie_seen)?;
            if path != "/api/s/default/stat/device" {
                return Err(format!("expected device lookup, got {path}"));
            }
            Ok(())
        }
        2 => {
            require_cookie(headers, cookie_seen)?;
            if path != "/api/s/default/stat/sta" {
                return Err(format!("expected client lookup, got {path}"));
            }
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn require_cookie(headers: &str, cookie_seen: &bool) -> Result<(), String> {
    let has_cookie = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("cookie") && value.trim() == "SESSION=fixture"
    });
    if !*cookie_seen || !has_cookie {
        Err(format!("session cookie was not reused: {headers}"))
    } else {
        Ok(())
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nSet-Cookie: SESSION=fixture; Path=/\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write fixture response");
}

#[tokio::test]
async fn logs_in_reuses_cookie_looks_up_names_and_filters_clients_by_ap() {
    let fixture = start_fixture(false);
    let config = config_for(&fixture.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");
    let requested = vec![AP_ONE.to_owned(), AP_TWO.to_owned()];

    let batch = client
        .fetch_snapshots(&requested)
        .await
        .expect("successful fixture response should map");

    assert_eq!(batch.unavailable_aps, Vec::<String>::new());
    assert_eq!(batch.snapshots.len(), 2);
    assert_eq!(batch.snapshots[0].ap_name, "Living Room AP");
    assert_eq!(batch.snapshots[0].clients.len(), 1);
    assert_eq!(batch.snapshots[0].clients[0].mac, "AA:AA:AA:AA:AA:01");
    assert_eq!(batch.snapshots[1].ap_name, "Office AP");
    assert_eq!(batch.snapshots[1].clients.len(), 1);
    assert_eq!(batch.snapshots[1].clients[0].mac, "AA:AA:AA:AA:AA:02");

    for _ in 0..3 {
        fixture
            .requests
            .recv()
            .expect("fixture request result")
            .expect("request should match");
    }
    fixture.thread.join().expect("fixture thread should finish");
}

#[tokio::test]
async fn keeps_known_snapshots_and_reports_only_missing_requested_aps() {
    let fixture = start_fixture(false);
    let config = config_for(&fixture.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");
    let missing = "44:55:66:77:88:99".to_owned();
    let requested = vec![AP_ONE.to_owned(), missing.clone()];

    let batch = client
        .fetch_snapshots(&requested)
        .await
        .expect("missing AP should be reported in the batch");

    assert_eq!(batch.snapshots.len(), 1);
    assert_eq!(batch.snapshots[0].ap_mac, AP_ONE);
    assert_eq!(batch.unavailable_aps, vec![missing]);

    for _ in 0..3 {
        fixture
            .requests
            .recv()
            .expect("fixture request result")
            .expect("request should match");
    }
    fixture.thread.join().expect("fixture thread should finish");
}

#[tokio::test]
async fn returns_an_error_when_client_api_reports_failure() {
    let fixture = start_fixture(true);
    let config = config_for(&fixture.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");
    let requested = vec![AP_ONE.to_owned()];

    assert!(client.fetch_snapshots(&requested).await.is_err());

    for _ in 0..3 {
        fixture
            .requests
            .recv()
            .expect("fixture request result")
            .expect("request should match");
    }
    fixture.thread.join().expect("fixture thread should finish");
}

#[tokio::test]
async fn logs_in_again_and_retries_once_when_the_session_expires() {
    let fixture = start_expired_session_fixture();
    let config = config_for(&fixture.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");

    let batch = client
        .fetch_snapshots(&[AP_ONE.to_owned()])
        .await
        .expect("expired session should be renewed and request retried");

    assert_eq!(batch.snapshots.len(), 1);
    assert_eq!(batch.snapshots[0].ap_name, "Living Room AP");
    assert!(batch.unavailable_aps.is_empty());
    let paths = (0..5)
        .map(|_| {
            fixture
                .requests
                .recv()
                .expect("fixture request result")
                .expect("request should match")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "/api/login",
            "/api/s/default/stat/device",
            "/api/login",
            "/api/s/default/stat/device",
            "/api/s/default/stat/sta"
        ]
    );
    fixture.thread.join().expect("fixture thread should finish");
}

#[tokio::test]
async fn returns_http_status_errors_from_login_and_statistics_requests() {
    let failed_login = start_status_fixture(503, 200);
    let config = config_for(&failed_login.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");
    let error = client
        .fetch_snapshots(&[AP_ONE.to_owned()])
        .await
        .expect_err("non-success login status should be returned");
    assert!(error.to_string().contains("503"));
    failed_login
        .requests
        .recv()
        .expect("login request should arrive")
        .expect("login request should match");
    failed_login
        .thread
        .join()
        .expect("login status fixture should finish");

    let failed_statistics = start_status_fixture(200, 503);
    let config = config_for(&failed_statistics.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");
    let error = client
        .fetch_snapshots(&[AP_ONE.to_owned()])
        .await
        .expect_err("non-success statistics status should be returned");
    assert!(error.to_string().contains("503"));
    for _ in 0..2 {
        failed_statistics
            .requests
            .recv()
            .expect("fixture request should arrive")
            .expect("fixture request should match");
    }
    failed_statistics
        .thread
        .join()
        .expect("statistics status fixture should finish");
}

#[tokio::test]
async fn accepts_success_payloads_without_api_metadata() {
    let fixture = start_optional_metadata_fixture();
    let config = config_for(&fixture.base_url);
    let mut client = UniFiClient::new(&config).expect("client should build");

    let batch = client
        .fetch_snapshots(&[AP_ONE.to_owned()])
        .await
        .expect("payloads without meta.rc should remain compatible");

    assert_eq!(batch.snapshots.len(), 1);
    assert_eq!(batch.snapshots[0].ap_name, "Fixture AP");
    assert!(batch.snapshots[0].clients.is_empty());
    for _ in 0..3 {
        fixture
            .requests
            .recv()
            .expect("fixture request result")
            .expect("request should match");
    }
    fixture.thread.join().expect("fixture thread should finish");
}
