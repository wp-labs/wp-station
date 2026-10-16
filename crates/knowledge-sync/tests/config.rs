use wp_station_knowledge_sync::AssetSyncConfig;
use wp_station_knowledge_sync::geo::GeoSyncConfig;
use wp_station_knowledge_sync::intel::IntelSyncConfig;

#[test]
fn shared_target_url_supplies_asset_and_geo_connections() {
    let mmdb_dir = tempfile::tempdir().expect("创建 MMDB 测试目录");
    unsafe {
        std::env::set_var(
            "WP_KSYNC_TARGET_URL",
            "postgres://station:p%40ss%3Aword@127.0.0.1:5543/postgres",
        );
        std::env::set_var("WP_KSYNC_ASSET_SOURCE_DATABASE", "dysec-asset");
        std::env::set_var("WP_KSYNC_ASSET_CRON", "*/5 * * * *");
        std::env::set_var("WP_KSYNC_MMDB_DIR", mmdb_dir.path());
        std::env::set_var("WP_KSYNC_DORIS_HOST", "127.0.0.1");
        std::env::set_var("WP_KSYNC_DORIS_DATABASE", "ignored");
        std::env::set_var("WP_KSYNC_REDIS_URL", "redis://127.0.0.1:6379/0");
        for name in [
            "WP_KSYNC_ASSET_SOURCE_HOST",
            "WP_KSYNC_ASSET_SOURCE_PORT",
            "WP_KSYNC_ASSET_SOURCE_USERNAME",
            "WP_KSYNC_ASSET_SOURCE_PASSWORD",
        ] {
            std::env::remove_var(name);
        }
    }

    let asset = AssetSyncConfig::from_env()
        .expect("解析资产配置")
        .expect("资产已配置");
    assert_eq!(asset.source_host, "127.0.0.1");
    assert_eq!(asset.source_port, 5543);
    assert_eq!(asset.source_username, "station");
    assert_eq!(asset.source_password, "p@ss:word");
    assert_eq!(asset.source_database, "dysec-asset");

    let geo = GeoSyncConfig::from_env()
        .expect("解析 Geo 配置")
        .expect("Geo 已配置");
    assert_eq!(geo.target_database_url, asset.target_database_url);
    assert_eq!(geo.mmdb_dir, mmdb_dir.path());

    let intel = IntelSyncConfig::from_env()
        .expect("解析情报配置")
        .expect("情报已配置");
    assert_eq!(intel.doris_database, "intel");
}
