use std::env;

use wp_station::server::start;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    let show_version = env::args().any(|arg| arg == "--version" || arg == "-V");

    if show_version {
        println!("wp-station {}", APP_VERSION);
        return;
    }

    start().await.expect("启动服务器失败");
}
