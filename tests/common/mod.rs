use rand::Rng;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::OnceCell;
use wp_station::db::{get_pool, init_default_configs_to_project};
use wp_station::{Setting, init_pool};

static SETTINGS: OnceCell<Setting> = OnceCell::const_new();
static MIGRATIONS_DONE: OnceCell<()> = OnceCell::const_new();
static TEST_PROJECT_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn init_test_environment() {
    TEST_PROJECT_ROOT.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("wp-station-tests-{}", std::process::id()));
        let project_dir = base.join("project-root");
        let _ = std::fs::remove_dir_all(&project_dir);
        std::fs::create_dir_all(&project_dir).expect("failed to create test project root");
        unsafe {
            std::env::set_var("WP_STATION__PROJECT_ROOT", &project_dir);
            std::env::set_var("WARP_STATION_SKIP_GITEA", "1");
            std::env::set_var("WARP_STATION_SKIP_RULE_CHECK", "1");
            std::env::set_var("WARP_STATION_SKIP_SANDBOX", "1");
        }
        project_dir
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

pub fn test_project_root() -> PathBuf {
    init_test_environment();
    TEST_PROJECT_ROOT
        .get()
        .expect("test project root initialized")
        .clone()
}

pub fn remove_project_path(relative: impl AsRef<std::path::Path>) {
    let path = test_project_root().join(relative);
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
    let project_root = test_project_root();
    let _ = fs::remove_dir_all(&project_root);
    fs::create_dir_all(&project_root).expect("recreate test project root");
    init_default_configs_to_project(project_root.to_str().expect("utf-8 test project root"))
        .expect("initialize default configs to test project root");
}
