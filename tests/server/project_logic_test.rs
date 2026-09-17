use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{
    rand_suffix, setup_db, test_base_root, test_infra_root, test_models_root, test_project_layout,
};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use wp_station::db::{
    ReleaseStatus, find_all_releases, find_latest_draft_release, update_release_status,
};
use wp_station::server::project::{
    ProjectImportRequest, confirm_project_archive_import_logic, export_project_archive_logic,
    import_project_from_files_logic, preview_project_archive_logic,
};
use wp_station::utils::SystemKind;
use wp_station::utils::compose_repo_layout_into;

fn legacy_import_dir(name: &str) -> PathBuf {
    let path = test_base_root().join(format!("legacy-import-{}-{}", name, rand_suffix()));
    fs::create_dir_all(&path).expect("create legacy import dir");
    path
}

fn write_file(path: PathBuf, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write file");
}

fn copy_dir(source: PathBuf, target: PathBuf) {
    fs::create_dir_all(&target).expect("create target dir");
    for entry in fs::read_dir(source).expect("read source dir") {
        let entry = entry.expect("read source entry");
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir(source_path, target_path);
        } else {
            fs::copy(&source_path, &target_path).expect("copy file");
        }
    }
}

fn build_archive_with_dirs(source_dir: &Path, dirs: &[&str]) -> Vec<u8> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for dir in dirs {
        builder
            .append_dir_all(*dir, source_dir.join(dir))
            .expect("append dir to archive");
    }
    let encoder = builder.into_inner().expect("finish tar builder");
    encoder.finish().expect("finish gzip encoder")
}

fn archive_entry_names(bytes: &[u8]) -> Vec<String> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive
        .entries()
        .expect("read archive entries")
        .map(|entry| {
            entry
                .expect("read archive entry")
                .path()
                .expect("resolve archive entry path")
                .to_string_lossy()
                .to_string()
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn assert_archive_has_flat_root_dirs(entries: &[String]) {
    assert!(
        entries.iter().any(|item| item == "wp-station-project/conf"
            || item.starts_with("wp-station-project/conf/")),
        "archive should contain conf root: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|item| item == "wp-station-project/connectors"
                || item.starts_with("wp-station-project/connectors/")),
        "archive should contain connectors root: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|item| item == "wp-station-project/topology"
                || item.starts_with("wp-station-project/topology/")),
        "archive should contain topology root: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|item| item == "wp-station-project/models"
                || item.starts_with("wp-station-project/models/")),
        "archive should contain models root: {entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|item| item.starts_with("wp-station-project/project_models")),
        "archive should not contain project_models wrapper: {entries:?}"
    );
    assert!(
        !entries
            .iter()
            .any(|item| item.starts_with("wp-station-project/project_infra")),
        "archive should not contain project_infra wrapper: {entries:?}"
    );
}

#[tokio::test]
async fn test_import_project_requires_legacy_directories() {
    setup_db().await;
    let source_dir = legacy_import_dir("missing-dirs");
    fs::create_dir_all(source_dir.join("conf")).expect("create conf only");

    let result = import_project_from_files_logic(ProjectImportRequest {
        system: SystemKind::Wparse,
        source_dir: source_dir.to_string_lossy().to_string(),
    })
    .await;

    let err = match result {
        Ok(_) => panic!("should reject incomplete legacy directory"),
        Err(err) => err,
    };
    assert!(format!("{err}").contains("缺少必要目录"));

    let _ = fs::remove_dir_all(source_dir);
}

#[tokio::test]
async fn test_import_project_validates_source_dir_before_overwrite() {
    setup_db().await;
    let source_dir = legacy_import_dir("invalid-components");
    compose_repo_layout_into(&test_project_layout(), &source_dir)
        .expect("compose dual repo into legacy project");

    write_file(
        source_dir.join("topology/sources/wpsrc.toml"),
        "[[sources]]\nkey = \"gen_udp\"\nenable = true\nconnect = \"broken\"\n",
    );
    write_file(test_infra_root().join("sentinel.txt"), "keep infra");
    write_file(test_models_root().join("sentinel.txt"), "keep models");

    let result = import_project_from_files_logic(ProjectImportRequest {
        system: SystemKind::Wparse,
        source_dir: source_dir.to_string_lossy().to_string(),
    })
    .await;

    let err = match result {
        Ok(_) => panic!("should fail on source component validation"),
        Err(err) => err,
    };
    assert!(
        format!("{err}").contains("加载项目失败") || format!("{err}").contains("组件校验失败"),
        "unexpected error: {err}"
    );
    assert_eq!(
        fs::read_to_string(test_infra_root().join("sentinel.txt")).expect("read infra sentinel"),
        "keep infra"
    );
    assert_eq!(
        fs::read_to_string(test_models_root().join("sentinel.txt")).expect("read models sentinel"),
        "keep models"
    );

    let _ = fs::remove_dir_all(source_dir);
}

#[tokio::test]
async fn test_import_project_splits_legacy_directory_into_dual_repos() {
    setup_db().await;
    let source_dir = legacy_import_dir("success");
    compose_repo_layout_into(&test_project_layout(), &source_dir)
        .expect("compose dual repo into legacy project");

    write_file(
        source_dir.join("topology/sources/wpsrc.toml"),
        "[[sources]]\nkey = \"gen_udp\"\nenable = true\nconnect = \"syslog_udp_src\"\n[sources.params]\nport = 31609\n",
    );

    write_file(test_models_root().join("stale.txt"), "old models");
    write_file(test_infra_root().join("stale.txt"), "old infra");

    let response = import_project_from_files_logic(ProjectImportRequest {
        system: SystemKind::Wparse,
        source_dir: source_dir.to_string_lossy().to_string(),
    })
    .await
    .expect("import legacy project");

    assert_eq!(
        response.summary.source_dir,
        source_dir.to_string_lossy().to_string()
    );
    assert_eq!(
        response.summary.models_root,
        test_models_root().to_string_lossy().to_string()
    );
    assert_eq!(
        response.summary.infra_root,
        test_infra_root().to_string_lossy().to_string()
    );
    assert!(response.summary.rules_imported > 0);
    assert!(response.validation.passed);
    assert!(response.summary.rule_breakdown.iter().any(
        |item| item.rule_type == "parse" && item.files.iter().any(|file| file == "wparse.toml")
    ));

    assert!(test_infra_root().join("conf/wparse.toml").exists());
    assert!(
        test_infra_root()
            .join("topology/sources/wpsrc.toml")
            .exists()
    );
    assert!(test_models_root().join("models/wpl").exists());
    assert!(test_models_root().join("models/knowledge").exists());
    assert!(!test_models_root().join("stale.txt").exists());
    assert!(!test_infra_root().join("stale.txt").exists());

    let _ = fs::remove_dir_all(source_dir);
}

#[tokio::test]
async fn test_import_project_archive_supports_models_only_directory() {
    setup_db().await;
    if let Some(draft) = find_latest_draft_release(SystemKind::Wparse)
        .await
        .expect("query draft before import")
    {
        update_release_status(draft.id, ReleaseStatus::INIT, None, None)
            .await
            .expect("archive existing draft before import");
    }
    let source_dir = legacy_import_dir("archive-models-only");
    copy_dir(test_models_root().join("models"), source_dir.join("models"));
    write_file(
        source_dir.join("models/archive-only.txt"),
        "archive models only",
    );
    write_file(test_infra_root().join("sentinel.txt"), "keep infra");

    let preview = preview_project_archive_logic(
        SystemKind::Wparse,
        "models-only.tar.gz",
        build_archive_with_dirs(&source_dir, &["models"]),
    )
    .await
    .expect("preview models-only archive");

    assert_eq!(preview.summary.imported_dirs, vec!["models".to_string()]);
    assert_eq!(
        preview.summary.retained_dirs,
        vec![
            "conf".to_string(),
            "connectors".to_string(),
            "topology".to_string()
        ]
    );
    assert!(
        preview.summary.rule_breakdown.iter().all(|item| !matches!(
            item.rule_type.as_str(),
            "parse" | "source" | "sink" | "source_connect" | "sink_connect"
        )),
        "models-only preview should not include retained infra items: {:?}",
        preview.summary.rule_breakdown
    );

    let response = confirm_project_archive_import_logic(SystemKind::Wparse, &preview.import_id)
        .await
        .expect("confirm models-only archive");

    assert_eq!(response.summary.imported_dirs, vec!["models".to_string()]);
    assert!(
        response.summary.rule_breakdown.iter().all(|item| !matches!(
            item.rule_type.as_str(),
            "parse" | "source" | "sink" | "source_connect" | "sink_connect"
        )),
        "models-only response should not include retained infra items: {:?}",
        response.summary.rule_breakdown
    );
    assert_eq!(
        fs::read_to_string(test_infra_root().join("sentinel.txt")).expect("read infra sentinel"),
        "keep infra"
    );
    assert_eq!(
        fs::read_to_string(test_models_root().join("models/archive-only.txt"))
            .expect("read imported models marker"),
        "archive models only"
    );
    let draft = find_latest_draft_release(SystemKind::Wparse)
        .await
        .expect("query draft after import")
        .expect("draft should be recreated after archive import");
    assert_eq!(draft.status, ReleaseStatus::WAIT.as_ref());
    let (releases, total) =
        find_all_releases(Some(SystemKind::Wparse), 1, 20, None, None, None, None)
            .await
            .expect("query release list after import");
    assert!(
        total >= 1,
        "expected at least one visible release after import"
    );
    assert!(
        releases.iter().any(|release| release.id == draft.id),
        "draft release should be visible in release list"
    );

    let _ = fs::remove_dir_all(source_dir);
}

#[tokio::test]
async fn test_import_project_archive_supports_conf_only_directory() {
    setup_db().await;
    let source_dir = legacy_import_dir("archive-conf-only");
    copy_dir(test_infra_root().join("conf"), source_dir.join("conf"));
    write_file(
        test_models_root().join("sentinel-models.txt"),
        "keep models",
    );

    let preview = preview_project_archive_logic(
        SystemKind::Wparse,
        "conf-only.tar.gz",
        build_archive_with_dirs(&source_dir, &["conf"]),
    )
    .await
    .expect("preview conf-only archive");

    assert_eq!(preview.summary.imported_dirs, vec!["conf".to_string()]);
    assert_eq!(
        preview.summary.retained_dirs,
        vec![
            "connectors".to_string(),
            "topology".to_string(),
            "models".to_string()
        ]
    );
    assert_eq!(preview.summary.rules_imported, 2);
    assert!(preview.summary.rule_breakdown.iter().any(|item| {
        item.rule_type == "parse" && item.files == vec!["wparse.toml".to_string()]
    }));
    assert!(
        preview.summary.rule_breakdown.iter().any(|item| {
            item.rule_type == "wpgen" && item.files == vec!["wpgen.toml".to_string()]
        })
    );

    let response = confirm_project_archive_import_logic(SystemKind::Wparse, &preview.import_id)
        .await
        .expect("confirm conf-only archive");

    assert_eq!(response.summary.imported_dirs, vec!["conf".to_string()]);
    assert_eq!(response.summary.rules_imported, 2);
    assert!(response.summary.rule_breakdown.iter().any(|item| {
        item.rule_type == "parse" && item.files == vec!["wparse.toml".to_string()]
    }));
    assert!(
        response.summary.rule_breakdown.iter().any(|item| {
            item.rule_type == "wpgen" && item.files == vec!["wpgen.toml".to_string()]
        })
    );
    assert_eq!(
        fs::read_to_string(test_models_root().join("sentinel-models.txt"))
            .expect("read models sentinel"),
        "keep models"
    );

    let _ = fs::remove_dir_all(source_dir);
}

#[tokio::test]
async fn test_export_project_archive_uses_flat_root_directories() {
    setup_db().await;

    let archive = export_project_archive_logic(SystemKind::Wparse)
        .await
        .expect("export project archive");
    let entries = archive_entry_names(&archive.bytes);

    let timestamp = archive
        .file_name
        .strip_prefix("wparse-")
        .and_then(|name| name.strip_suffix(".tar.gz"))
        .expect("wparse archive should use the wparse timestamp name");
    assert!(timestamp.parse::<i64>().is_ok());
    assert_archive_has_flat_root_dirs(&entries);
}

#[tokio::test]
async fn test_export_wfusion_project_archive_uses_flat_root_directories() {
    setup_db().await;

    let archive = export_project_archive_logic(SystemKind::Wfusion)
        .await
        .expect("export wfusion project archive");
    let entries = archive_entry_names(&archive.bytes);

    let timestamp = archive
        .file_name
        .strip_prefix("wfusion-")
        .and_then(|name| name.strip_suffix(".tar.gz"))
        .expect("wfusion archive should use the wfusion timestamp name");
    assert!(timestamp.parse::<i64>().is_ok());
    assert_archive_has_flat_root_dirs(&entries);
    assert!(
        entries
            .iter()
            .any(|item| item == "wp-station-project/conf/wfusion.toml"),
        "archive should contain wfusion config file: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|item| item == "wp-station-project/models/windows.toml"),
        "archive should contain wfusion windows config: {entries:?}"
    );
}

#[tokio::test]
async fn test_import_project_archive_wfusion_breakdown_keeps_virtual_named_rule_display() {
    setup_db().await;
    let source_dir = legacy_import_dir("archive-wfusion-models");
    fs::create_dir_all(source_dir.join("models/schemas")).expect("create schemas dir");
    fs::create_dir_all(source_dir.join("models/rules")).expect("create rules dir");
    fs::create_dir_all(source_dir.join("models/scenarios")).expect("create scenarios dir");
    write_file(
        source_dir.join("models/windows.toml"),
        r#"[window_defaults]
evict_interval = "30s"
max_window_bytes = "256MB"
max_total_bytes = "2GB"
evict_policy = "time_first"
watermark = "5s"
allowed_lateness = "8760h"
late_policy = "drop"
"#,
    );
    write_file(
        source_dir.join("models/schemas/kunai.wfs"),
        r#"window ssh_login {
    stream_tag = "ssh_login"
    time = occur_time
    over = 1m
    fields {
        occur_time: time
        source_ip: ip
        target_user: chars
        outcome: chars
    }
}

window security_alerts {
    over = 0
    fields {
        alert_name: chars
        source_ip: ip
        target_user: chars
        failed_count: digit
    }
}
"#,
    );
    write_file(
        source_dir.join("models/rules/sql_injection_source_alert.wfl"),
        r#"use "../schemas/kunai.wfs"

rule ssh_brute_force_alert {
    events {
        failed : ssh_login && outcome == "failed"
    }
    match<source_ip,target_user:1m:fixed> {
        on event { failed | count >= 3; }
    } -> score(80.0)
    entity(ip, failed.source_ip)
    yield security_alerts (
        alert_name = "SSH 暴力破解",
        source_ip = failed.source_ip,
        target_user = failed.target_user,
        failed_count = count(failed)
    )
    limits {
        max_instances = 1000;
    }
}
"#,
    );
    write_file(
        source_dir.join("models/scenarios/ssh_brute_force_attempt.wfg"),
        r#"use "../schemas/kunai.wfs"
use "../rules/sql_injection_source_alert.wfl"

#[duration=10s]
scenario ssh_brute_force_alert_case<seed=42> {
    traffic {
        stream ssh_login gen 1/s
    }
    injection {
        hit<100%> ssh_login {
            source_ip seq {
                use(source_ip="192.168.1.100", target_user="root", outcome="failed") with(3)
            }
        }
    }
    expect {
        hit(ssh_brute_force_alert) >= 100%
    }
}
"#,
    );

    let preview = preview_project_archive_logic(
        SystemKind::Wfusion,
        "wfusion-models.tar.gz",
        build_archive_with_dirs(&source_dir, &["models"]),
    )
    .await
    .expect("preview wfusion models archive");

    let schema_item = preview
        .summary
        .rule_breakdown
        .iter()
        .find(|item| item.rule_type == "schema")
        .expect("schema breakdown");
    assert_eq!(schema_item.files, vec!["kunai.wfs".to_string()]);

    let rule_item = preview
        .summary
        .rule_breakdown
        .iter()
        .find(|item| item.rule_type == "rule")
        .expect("rule breakdown");
    assert_eq!(
        rule_item.files,
        vec!["sql_injection_source_alert.wfl".to_string()]
    );

    let scenario_item = preview
        .summary
        .rule_breakdown
        .iter()
        .find(|item| item.rule_type == "scenarios")
        .expect("scenario breakdown");
    assert_eq!(
        scenario_item.files,
        vec!["ssh_brute_force_attempt.wfg".to_string()]
    );

    let windows_item = preview
        .summary
        .rule_breakdown
        .iter()
        .find(|item| item.rule_type == "windows")
        .expect("windows breakdown");
    assert_eq!(windows_item.files, vec!["windows.toml".to_string()]);

    let _ = fs::remove_dir_all(source_dir);
}
