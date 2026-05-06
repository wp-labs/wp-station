use crate::common::{rand_suffix, setup_db};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::server::release::{
    ReleaseListQuery, create_release_logic, get_release_detail_logic, list_releases_logic,
    validate_release_logic,
};
use wp_station::utils::pagination::PageQuery;
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};

async fn cleanup_release(version: &str) {
    let pool = wp_station::db::get_pool();
    let _ = ReleaseEntity::delete_many()
        .filter(ReleaseColumn::Version.eq(version))
        .exec(pool.inner())
        .await;
}

#[tokio::test]
async fn test_release_logic_flow() {
    setup_db().await;
    let requested_pipeline = format!("pipeline-{}", rand_suffix());

    let create_resp =
        create_release_logic(Some(requested_pipeline.clone()), Some("note".to_string()))
            .await
            .expect("create release logic");
    assert!(create_resp.success);

    let detail = get_release_detail_logic(create_resp.id)
        .await
        .expect("detail logic");
    let actual_version = detail.version.clone();

    let list_resp = list_releases_logic(ReleaseListQuery {
        note: None,
        pipeline: None,
        version: Some(actual_version.clone()),
        owner: None,
        created_by: None,
        status: Some("WAIT".to_string()),
        page: PageQuery {
            page: Some(1),
            page_size: Some(10),
        },
    })
    .await
    .expect("list releases logic");
    assert!(list_resp.items.iter().any(|r| r.id == create_resp.id));
    assert_eq!(detail.version, actual_version);

    let validate_resp = validate_release_logic(create_resp.id)
        .await
        .expect("validate release logic");
    assert!(validate_resp.valid);

    cleanup_release(&actual_version).await;
}
