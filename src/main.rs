//! `wp-station` 可执行程序入口。

use std::env;

use wp_station::server::start;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
/// 启动服务主入口；支持 `--version` 快速输出版本。
async fn main() {
    let show_version = env::args().any(|arg| arg == "--version" || arg == "-V");

    if show_version {
        println!("wp-station {}", APP_VERSION);
        return;
    }

    start().await.expect("启动服务器失败");
}
