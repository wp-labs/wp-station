use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::Utc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use wp_station::db::{Device, DeviceStatus};
use wp_station::server::setting::AdminApiConf;
use wp_station::utils::{PublishPayload, SystemKind, WfusionService};

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
                                None => ("404 Not Found", "{\"error\":\"not found\"}".to_string()),
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

fn mock_device(ip: &str, port: i32) -> Device {
    Device {
        id: 1,
        system: SystemKind::Wfusion.as_ref().to_string(),
        name: Some("wfusion-test".to_string()),
        ip: ip.to_string(),
        port,
        remark: None,
        status: DeviceStatus::Unknown.as_ref().to_string(),
        token: "123456".to_string(),
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
async fn test_wfusion_health_check_supports_unspecified_host() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/runtime/status".to_string(),
        "{\"instance_id\":\"fusion:1\",\"version\":\"0.1.20\",\"accepting\":true}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("0.0.0.0", server.port());
    let result = service
        .check_health(&device)
        .await
        .expect("wfusion health check");

    assert!(result.is_online);
    assert_eq!(result.client_version.as_deref(), Some("0.1.20"));
    assert_eq!(result.config_version, None);
}

#[tokio::test]
async fn test_wfusion_publish_supports_immediate_apply_response() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/reloads/model".to_string(),
        "{\"request_id\":\"req-1\",\"accepted\":true,\"result\":\"applied\",\"update\":true,\"current_version\":\"1.2.3\",\"resolved_tag\":\"v1.2.3\"}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("127.0.0.1", server.port());
    let result = service
        .publish(
            &device,
            PublishPayload {
                version: "1.2.3".to_string(),
                release_group: "models".to_string(),
            },
        )
        .await
        .expect("wfusion publish");

    assert!(result.accepted);
    assert!(result.completed);
    assert_eq!(result.request_id.as_deref(), Some("req-1"));
    assert_eq!(
        result.message.as_deref(),
        Some("result=applied, update=true, current_version=1.2.3, resolved_tag=v1.2.3")
    );
}

#[tokio::test]
async fn test_wfusion_publish_treats_restart_required_as_completed_result() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/reloads/model".to_string(),
        "{\"request_id\":\"req-3\",\"accepted\":true,\"result\":\"restart_required\",\"update\":true,\"requested_version\":\"1.2.3\",\"current_version\":\"1.2.3\",\"resolved_tag\":\"v1.2.3\"}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("127.0.0.1", server.port());
    let result = service
        .publish(
            &device,
            PublishPayload {
                version: "1.2.3".to_string(),
                release_group: "infra".to_string(),
            },
        )
        .await
        .expect("wfusion publish");

    assert!(result.accepted);
    assert!(result.completed);
    assert_eq!(result.request_id.as_deref(), Some("req-3"));
    assert_eq!(
        result.message.as_deref(),
        Some(
            "result=restart_required, update=true, requested_version=1.2.3, current_version=1.2.3, resolved_tag=v1.2.3"
        )
    );
}

#[tokio::test]
async fn test_wfusion_publish_rejects_response_with_error_field() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/reloads/model".to_string(),
        "{\"request_id\":\"req-4\",\"accepted\":true,\"result\":\"restart_required\",\"update\":true,\"error\":\"configuration error\"}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("127.0.0.1", server.port());
    let result = service
        .publish(
            &device,
            PublishPayload {
                version: "1.2.3".to_string(),
                release_group: "infra".to_string(),
            },
        )
        .await
        .expect("wfusion publish");

    assert!(!result.accepted);
    assert!(!result.completed);
    assert_eq!(result.request_id.as_deref(), Some("req-4"));
    assert_eq!(
        result.message.as_deref(),
        Some("result=restart_required, error=configuration error, update=true")
    );
}

#[tokio::test]
async fn test_wfusion_publish_status_check_accepts_matching_request_id() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/runtime/status".to_string(),
        "{\"version\":\"0.1.20\",\"accepting\":true,\"reloading\":false,\"last_reload_request_id\":\"req-2\",\"last_reload_result\":\"applied\",\"config_version\":\"v1.2.3\"}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("127.0.0.1", server.port());
    let result = service
        .check_deploy_success(&device, "1.2.3", Some("req-2"))
        .await
        .expect("wfusion publish status check");

    assert!(result.is_success);
    assert_eq!(result.current_version.as_deref(), Some("v1.2.3"));
    assert_eq!(result.config_version.as_deref(), Some("v1.2.3"));
    assert!(!result.is_reloading);
}

#[tokio::test]
async fn test_wfusion_publish_status_check_accepts_restart_required_without_config_version() {
    let server = MockHttpServer::start(HashMap::from([(
        "/admin/v1/runtime/status".to_string(),
        "{\"version\":\"0.1.29\",\"accepting\":true,\"reloading\":false,\"last_reload_request_id\":\"req-5\",\"last_reload_result\":\"restart_required\"}".to_string(),
    )]))
    .await;

    let service =
        WfusionService::from_admin_api_conf(&http_conf(), None).expect("create wfusion service");
    let device = mock_device("127.0.0.1", server.port());
    let result = service
        .check_deploy_success(&device, "1.2.3", Some("req-5"))
        .await
        .expect("wfusion publish status check");

    assert!(result.is_success);
    assert_eq!(result.current_version, None);
    assert_eq!(result.config_version, None);
    assert!(!result.is_reloading);
}
