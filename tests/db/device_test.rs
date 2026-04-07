use crate::common::{rand_suffix, setup_db, unique_name};
use rand::Rng;
use sea_orm::EntityTrait;
use wp_station::db::{
    DeviceStatus, NewDevice, create_device, delete_device, find_all_devices, find_device_by_id,
    find_devices_page, update_device, update_device_status,
};
use wp_station_migrations::entity::device::Entity as DeviceEntity;

fn make_device_payload() -> NewDevice {
    let mut rng = rand::thread_rng();
    let ip = format!(
        "10.{}.{}.{}",
        rng.gen_range(0..=250),
        rng.gen_range(0..=250),
        rng.gen_range(1..=250)
    );

    NewDevice {
        name: Some(unique_name("device")),
        ip,
        port: rng.gen_range(1000..9999),
        remark: Some(format!("test-{}", rand_suffix())),
        token: format!("token-{}", rand_suffix()),
        status: Some(DeviceStatus::Unknown),
    }
}

async fn insert_device_with_payload(payload: NewDevice) -> (i32, NewDevice) {
    setup_db().await;
    let id = create_device(payload.clone()).await.expect("create device");
    (id, payload)
}

async fn insert_device() -> (i32, NewDevice) {
    insert_device_with_payload(make_device_payload()).await
}

async fn hard_delete_device(id: i32) {
    let pool = wp_station::db::get_pool();
    let _ = DeviceEntity::delete_by_id(id).exec(pool.inner()).await;
}

#[tokio::test]
async fn test_create_device() {
    let (device_id, _) = insert_device().await;
    assert!(device_id > 0);
    let _ = delete_device(device_id).await;
    hard_delete_device(device_id).await;
}

#[tokio::test]
async fn test_find_device_by_id() {
    let (device_id, payload) = insert_device().await;

    let found = find_device_by_id(device_id)
        .await
        .expect("find device")
        .expect("device exists");

    assert_eq!(found.id, device_id);
    assert_eq!(found.ip, payload.ip);
    assert_eq!(found.port, payload.port);
    assert_eq!(found.name, payload.name);

    let _ = delete_device(device_id).await;
    hard_delete_device(device_id).await;
}

#[tokio::test]
async fn test_delete_device() {
    let (device_id, _) = insert_device().await;

    delete_device(device_id).await.expect("delete device");

    let missing = find_device_by_id(device_id)
        .await
        .expect("find device after delete");
    assert!(missing.is_none(), "deleted devices should be filtered out");

    hard_delete_device(device_id).await;
}

#[tokio::test]
async fn test_find_devices_page_with_keyword() {
    let prefix = format!("page-{}", rand_suffix());
    let payload_one = NewDevice {
        name: Some(format!("{prefix}-a")),
        ..make_device_payload()
    };
    let payload_two = NewDevice {
        name: Some(format!("{prefix}-b")),
        ..make_device_payload()
    };

    let (first_id, first_payload) = insert_device_with_payload(payload_one).await;
    let (second_id, second_payload) = insert_device_with_payload(payload_two).await;

    let (devices, total) = find_devices_page(Some(prefix.as_str()), 1, 10)
        .await
        .expect("page devices");

    assert!(total >= 2);
    let names: Vec<String> = devices.iter().filter_map(|d| d.name.clone()).collect();
    assert!(
        names
            .iter()
            .any(|name| Some(name) == first_payload.name.as_ref())
    );
    assert!(
        names
            .iter()
            .any(|name| Some(name) == second_payload.name.as_ref())
    );

    let _ = delete_device(first_id).await;
    let _ = delete_device(second_id).await;
    hard_delete_device(first_id).await;
    hard_delete_device(second_id).await;
}

#[tokio::test]
async fn test_update_device() {
    let (device_id, _) = insert_device().await;
    let mut new_payload = make_device_payload();
    new_payload.name = Some(unique_name("updated"));
    new_payload.remark = Some("updated-remark".to_string());
    new_payload.port = 5555;

    update_device(device_id, new_payload.clone())
        .await
        .expect("update device");

    let updated = find_device_by_id(device_id)
        .await
        .expect("find updated")
        .expect("exists");
    assert_eq!(updated.name, new_payload.name);
    assert_eq!(updated.port, new_payload.port);
    assert_eq!(updated.remark, new_payload.remark);

    delete_device(device_id).await.ok();
    hard_delete_device(device_id).await;
}

#[tokio::test]
async fn test_update_device_status_and_find_all() {
    let (device_id, _) = insert_device().await;

    update_device_status(device_id, DeviceStatus::Active)
        .await
        .expect("update status");

    let active = find_device_by_id(device_id)
        .await
        .expect("find after status update")
        .expect("exists");
    assert_eq!(active.status, DeviceStatus::Active.as_ref());

    delete_device(device_id).await.expect("soft delete");
    let all_devices = find_all_devices().await.expect("all devices");
    assert!(
        all_devices.iter().all(|d| d.id != device_id),
        "soft-deleted device should be filtered"
    );
    hard_delete_device(device_id).await;
}
