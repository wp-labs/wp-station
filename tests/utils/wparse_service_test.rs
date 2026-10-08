use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::Utc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use wp_station::db::{Device, DeviceStatus, ReleaseGroup};
use wp_station::server::setting::AdminApiConf;
use wp_station::utils::{SystemKind, WarpParseService};

struct MockHttpServer {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl MockHttpServer {
    async fn start(routes: HashMap<String, String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock listener");
        let addr = listener.local_addr().expect("mock listener addr");
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let (mut socket, _) = match accepted {
                            Ok(value) => value,
                            Err(_) => break,
                        };
                        let routes = routes.clone();
                        tokio::spawn(async move {
                            let mut buffer = [0u8; 8192];
                            let size = socket.read(&mut buffer).await.unwrap_or(0);
                            if size == 0 {
                                return;
                            }

                            let request = String::from_utf8_lossy(&buffer[..size]);
                            let path = request
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .unwrap_or("/");
                            let (status, body) = match routes.get(path) {
                                Some(body) => ("200 OK", body.clone()),
                                None => ("404 Not Found", "{}".to_string()),
                            };
                            let response = format!(
                                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        });
                    }
                }
            }
        });

        Self {
            addr,
            shutdown_tx: Some(shutdown_tx),
            handle,
        }
    }

    fn port(&self) -> i32 {
        self.addr.port() as i32
    }
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.handle.abort();
    }
}

fn mock_device(port: i32) -> Device {
    Device {
        id: 1,
        system: SystemKind::Wparse.as_ref().to_string(),
        name: Some("wparse-test".to_string()),
        ip: "127.0.0.1".to_string(),
        port,
        remark: None,
        status: DeviceStatus::Unknown.as_ref().to_string(),
        token: "test-token".to_string(),
        client_version: None,
        config_version: None,
        health_error: None,
        last_release_id: None,
        last_seen_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn http_conf() -> AdminApiConf {
    AdminApiConf {
        enabled: false,
        ca_file: String::new(),
        poll_interval_seconds: 5,
        poll_timeout_seconds: 300,
        max_retries: 60,
    }
}

#[tokio::test]
async fn test_wparse_status_queries_grouped_legacy_project_versions() {
    let project_version = "models=v1.0.4, infra=v1.0.4";
    let body = format!(
        r#"{{"version":"0.26.0-E2","project_version":"{project_version}","accepting_commands":true,"reloading":false,"last_reload_request_id":"req-1","last_reload_result":"reload_done"}}"#
    );
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/runtime/status".to_string(),
        body,
    )]))
    .await;
    let service = WarpParseService::from_admin_api_conf(&http_conf(), None)
        .expect("create WarpParse service");
    let device = mock_device(server.port());

    let models = service
        .check_deploy_success(&device, "1.0.4", ReleaseGroup::Models, Some("req-1"))
        .await
        .expect("query models deploy status");
    assert!(models.is_success);
    assert_eq!(models.current_version.as_deref(), Some("v1.0.4"));

    let infra = service
        .check_deploy_success(&device, "1.0.4", ReleaseGroup::Infra, Some("req-1"))
        .await
        .expect("query infra deploy status");
    assert!(infra.is_success);
    assert_eq!(infra.current_version.as_deref(), Some("v1.0.4"));

    let health = service
        .check_online(&device)
        .await
        .expect("query device health status");
    assert!(health.is_online);
    assert_eq!(health.client_version.as_deref(), Some("0.26.0-E2"));
    assert_eq!(health.config_version.as_deref(), Some(project_version));
}
