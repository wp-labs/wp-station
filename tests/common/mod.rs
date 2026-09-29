use rand::RngExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::OnceCell;
use wp_station::db::get_pool;
use wp_station::server::RepoLayout;
use wp_station::utils::{
    SystemKind, init_default_configs_to_infra_for_system,
    init_default_configs_to_models_for_system, layout_for_system,
};
use wp_station::{Setting, init_pool};

static SETTINGS: OnceCell<Setting> = OnceCell::const_new();
static MIGRATIONS_DONE: OnceCell<()> = OnceCell::const_new();
static TEST_BASE_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn init_test_environment() {
    TEST_BASE_ROOT.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("wp-station-tests-{}", std::process::id()));
        let sqlite_db = base.join("station-test.db");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("failed to create test workspace root");
        std::fs::File::create(&sqlite_db).expect("failed to create test sqlite database file");
        unsafe {
            std::env::set_var(
                "WP_STATION__DATABASE__URL",
                format!("sqlite://{}", sqlite_db.display()),
            );
            std::env::set_var("WP_STATION__DATABASE__MAX_CONNECTIONS", "1");
            std::env::set_var("WP_STATION__DATABASE__MIN_CONNECTIONS", "1");
            std::env::set_var("WARP_STATION_SKIP_GITEA", "1");
            std::env::set_var("WARP_STATION_SKIP_RULE_CHECK", "1");
            std::env::set_var("WARP_STATION_SKIP_SANDBOX", "1");
            std::env::set_var("WP_STATION_TEST_WORKSPACE_ROOT", &base);
        }
        base
    });
}

pub async fn setup_db() {
    init_test_environment();

    let setting = SETTINGS
        .get_or_init(|| async {
            init_test_environment();
            Setting::load()
        })
        .await
        .clone();

    // Always reinitialize the pool so each test owns a fresh Sqlx connection
    init_pool(&setting.database)
        .await
        .expect("failed to initialize database pool");

    MIGRATIONS_DONE
        .get_or_init(|| async {
            let pool = get_pool();
            use wp_station_migrations::{Migrator, MigratorTrait};
            Migrator::up(pool.inner(), None)
                .await
                .expect("failed to run migrations");
        })
        .await;

    cleanup_test_artifacts().await;
}

pub fn test_base_root() -> PathBuf {
    init_test_environment();
    TEST_BASE_ROOT
        .get()
        .expect("test base root initialized")
        .clone()
}

pub fn test_models_root() -> PathBuf {
    init_test_environment();
    layout_for_system(SystemKind::Wparse).models_root
}

pub fn test_infra_root() -> PathBuf {
    init_test_environment();
    layout_for_system(SystemKind::Wparse).infra_root
}

pub fn test_connectors_root() -> PathBuf {
    test_infra_root()
}

pub fn test_project_layout() -> RepoLayout {
    RepoLayout {
        models_root: test_models_root(),
        infra_root: test_infra_root(),
        connectors_root: test_connectors_root(),
    }
}

pub fn init_default_configs_to_test_layout() {
    let wparse_layout = layout_for_system(SystemKind::Wparse);
    let wfusion_layout = layout_for_system(SystemKind::Wfusion);
    for layout in [wparse_layout, wfusion_layout] {
        init_default_configs_to_models_for_system(
            layout.system,
            layout.models_root.to_str().expect("utf-8 test models root"),
        )
        .expect("initialize default configs to test models root");
        init_default_configs_to_infra_for_system(
            layout.system,
            layout.infra_root.to_str().expect("utf-8 test infra root"),
        )
        .expect("initialize default configs to test infra root");
    }
}

pub fn resolve_project_path(relative: impl AsRef<Path>) -> PathBuf {
    let relative = relative.as_ref();
    if relative.is_absolute() {
        return relative.to_path_buf();
    }

    let mut components = relative.components();
    let first = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .unwrap_or_default();

    match first {
        "conf" | "topology" => test_infra_root().join(relative),
        "connectors" => test_connectors_root().join(relative),
        ".run" | "models" => test_models_root().join(relative),
        _ => test_models_root().join(relative),
    }
}

pub fn remove_project_path(relative: impl AsRef<std::path::Path>) {
    let path = resolve_project_path(relative);
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

pub fn rand_suffix() -> String {
    rand::rng().random_range(10_000..99_999).to_string()
}

pub fn unique_name(prefix: &str) -> String {
    format!("{prefix}-{}", rand_suffix())
}

async fn cleanup_test_artifacts() {
    let wparse_layout = layout_for_system(SystemKind::Wparse);
    let wfusion_layout = layout_for_system(SystemKind::Wfusion);
    for layout in [&wparse_layout, &wfusion_layout] {
        let _ = fs::remove_dir_all(&layout.models_root);
        let _ = fs::remove_dir_all(&layout.infra_root);
        fs::create_dir_all(&layout.models_root).expect("recreate test models root");
        fs::create_dir_all(&layout.infra_root).expect("recreate test infra root");
    }
    init_default_configs_to_test_layout();
}
