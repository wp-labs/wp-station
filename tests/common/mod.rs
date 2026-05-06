use rand::Rng;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::OnceCell;
use wp_station::db::{get_pool, init_default_configs_to_infra, init_default_configs_to_models};
use wp_station::server::ProjectLayout;
use wp_station::{Setting, init_pool};

static SETTINGS: OnceCell<Setting> = OnceCell::const_new();
static MIGRATIONS_DONE: OnceCell<()> = OnceCell::const_new();
static TEST_BASE_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn init_test_environment() {
    TEST_BASE_ROOT.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("wp-station-tests-{}", std::process::id()));
        let models_dir = base.join("project_models");
        let infra_dir = base.join("project_infra");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&models_dir).expect("failed to create test models root");
        std::fs::create_dir_all(&infra_dir).expect("failed to create test infra root");
        unsafe {
            std::env::set_var("WP_STATION__PROJECT_MODELS", &models_dir);
            std::env::set_var("WP_STATION__PROJECT_INFRA", &infra_dir);
            std::env::set_var("WARP_STATION_SKIP_GITEA", "1");
            std::env::set_var("WARP_STATION_SKIP_RULE_CHECK", "1");
            std::env::set_var("WARP_STATION_SKIP_SANDBOX", "1");
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
    test_base_root().join("project_models")
}

pub fn test_infra_root() -> PathBuf {
    test_base_root().join("project_infra")
}

pub fn test_project_layout() -> ProjectLayout {
    ProjectLayout {
        models_root: test_models_root(),
        infra_root: test_infra_root(),
    }
}

pub fn init_default_configs_to_test_layout() {
    let models_root = test_models_root();
    let infra_root = test_infra_root();
    init_default_configs_to_models(models_root.to_str().expect("utf-8 test models root"))
        .expect("initialize default configs to test models root");
    init_default_configs_to_infra(infra_root.to_str().expect("utf-8 test infra root"))
        .expect("initialize default configs to test infra root");
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
        "conf" | "topology" | "connectors" => test_infra_root().join(relative),
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
    rand::thread_rng().gen_range(10_000..99_999).to_string()
}

pub fn unique_name(prefix: &str) -> String {
    format!("{prefix}-{}", rand_suffix())
}

async fn cleanup_test_artifacts() {
    let models_root = test_models_root();
    let infra_root = test_infra_root();
    let _ = fs::remove_dir_all(&models_root);
    let _ = fs::remove_dir_all(&infra_root);
    fs::create_dir_all(&models_root).expect("recreate test models root");
    fs::create_dir_all(&infra_root).expect("recreate test infra root");
    init_default_configs_to_test_layout();
}
