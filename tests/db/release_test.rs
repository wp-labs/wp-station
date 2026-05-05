use crate::common::{rand_suffix, setup_db};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{
    NewRelease, ReleaseGroup, ReleaseStatus, create_release, find_all_releases,
    find_latest_draft_release, find_release_by_id, update_release_pipeline, update_release_status,
};
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};

async fn cleanup_releases(prefix: &str) {
    let pool = wp_station::db::get_pool();
    let _ = ReleaseEntity::delete_many()
        .filter(ReleaseColumn::Version.like(&format!("%{}%", prefix)))
        .exec(pool.inner())
        .await;
}

#[tokio::test]
async fn test_release_crud_flow() {
    setup_db().await;
    let prefix = format!("REL-{}", rand_suffix());
    let version = format!("{}-v0", prefix);

    let release = NewRelease {
        version: version.clone(),
        release_group: ReleaseGroup::Models.as_ref().to_string(),
        pipeline: Some("auto".to_string()),
        created_by: Some("tester".to_string()),
        stages: Some("[]".to_string()),
        status: Some(ReleaseStatus::WAIT),
    };

    let release_id = create_release(release).await.expect("create release");

    let fetched = find_release_by_id(release_id)
        .await
        .expect("find release")
        .expect("release exists");
    assert_eq!(fetched.version, version);

    update_release_status(release_id, ReleaseStatus::PASS, Some("ok"), None)
        .await
        .expect("update status");

    let (items, total) = find_all_releases(
        1,
        10,
        Some("auto"),
        Some(&prefix),
        Some("tester"),
        Some(ReleaseStatus::PASS.as_ref()),
    )
    .await
    .expect("list releases");
    assert!(total >= 1);
    assert!(items.iter().any(|item| item.id == release_id));

    update_release_pipeline(release_id, Some("auto-updated"))
        .await
        .expect("update pipeline");

    let draft_version = format!("{}-draft", prefix);
    let draft_id = create_release(NewRelease {
        version: draft_version.clone(),
        release_group: "draft".to_string(),
        pipeline: Some("draft".to_string()),
        created_by: Some("tester".to_string()),
        stages: None,
        status: Some(ReleaseStatus::WAIT),
    })
    .await
    .expect("create draft");

    let draft = find_latest_draft_release()
        .await
        .expect("find draft")
        .expect("draft exists");
    assert_eq!(draft.id, draft_id);

    cleanup_releases(&prefix).await;
}
