use rand::Rng;
use sea_orm::Condition;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::sync::OnceCell;
use wp_station::db::get_pool;
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
            std::env::set_var("WARP_STATION_PROJECT_ROOT", &project_dir);
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

pub fn rand_suffix() -> String {
    rand::thread_rng().gen_range(10_000..99_999).to_string()
}

pub fn unique_name(prefix: &str) -> String {
    format!("{prefix}-{}", rand_suffix())
}

async fn cleanup_test_artifacts() {
    use wp_station_migrations::entity::knowledge_config::{Column, Entity};

    let pool = get_pool();
    let filter = Condition::any()
        .add(Column::FileName.like("api-knowledge-%"))
        .add(Column::FileName.like("debug-knowledge-%"));

    let _ = Entity::delete_many()
        .filter(filter)
        .exec(pool.inner())
        .await;
}
