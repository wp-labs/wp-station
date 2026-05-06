//! 沙盒模块集成测试。
//!
//! 覆盖工作区管理（输出收集、目录渲染、日志打包）和进程管理（命令检测、版本查询）。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use wp_station::server::Setting;
use wp_station::utils::common::OUTPUT_PATHS;
use wp_station::utils::sandbox::{SandboxWorkspace, collect_output_checks, command_version_output};

// ============ 测试辅助函数 ============

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}

fn temp_script(prefix: &str, contents: &str, ext: &str) -> PathBuf {
    let unique = format!(
        "{}-{}-{}{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        ext
    );
    let path = std::env::temp_dir().join(unique);
    fs::write(&path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    path
}

// ============ 工作区测试 ============

#[test]
fn collect_output_checks_counts_lines() {
    let base = temp_dir("collect-output");
    for (idx, (relative, _)) in OUTPUT_PATHS.iter().enumerate() {
        let path = base.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let content = if idx % 2 == 0 { "line1\nline2\n" } else { "" };
        fs::write(&path, content).unwrap();
    }

    let status = collect_output_checks(&base).expect("collect output succeeds");
    assert_eq!(status.len(), OUTPUT_PATHS.len());

    let mut map: HashMap<String, (usize, bool)> = HashMap::new();
    for item in status {
        map.insert(item.relative_path.clone(), (item.line_count, item.is_empty));
    }

    assert!(map.get("data/out_dat/default.dat").unwrap().0 >= 2);
    assert!(map.get("data/out_dat/miss.dat").unwrap().1);

    fs::remove_dir_all(&base).unwrap();
}

#[test]
fn display_relative_prefers_workspace_root() {
    let workspace_root = Setting::workspace_root().clone();
    let logs_dir = workspace_root.join("logs");
    let ws = SandboxWorkspace {
        root: workspace_root.clone(),
        project_dir: workspace_root.clone(),
        logs_dir,
        source_models_root: workspace_root.clone(),
        source_infra_root: workspace_root.clone(),
    };

    let nested = workspace_root.join("foo/bar/example.txt");
    let display = ws.display_relative(&nested);

    assert!(display.contains("foo"));
    assert!(!display.starts_with('/'));
    assert!(!display.starts_with('\\'));
}

#[test]
fn render_tree_listing_displays_structure() {
    let base = temp_dir("render-tree");
    let logs_dir = base.join("logs");
    fs::create_dir_all(&logs_dir).unwrap();
    let project_dir = base.join("project");
    fs::create_dir_all(project_dir.join("dir_a/dir_b")).unwrap();
    fs::write(project_dir.join("dir_a/file.txt"), "data").unwrap();

    let workspace = SandboxWorkspace {
        root: base.clone(),
        project_dir: project_dir.clone(),
        logs_dir,
        source_models_root: base.join("source-models"),
        source_infra_root: base.join("source-infra"),
    };

    let listing = workspace
        .render_tree_listing(3, 10)
        .expect("render tree listing");
    assert!(listing.contains("dir_a"));
    assert!(listing.contains("file.txt"));

    fs::remove_dir_all(&base).unwrap();
}

// ============ 进程管理测试 ============

#[tokio::test]
async fn command_version_output_reads_stdout() {
    #[cfg(windows)]
    let script = temp_script("version-ok", "@echo off\necho v9.9.9\n", ".cmd");
    #[cfg(not(windows))]
    let script = temp_script("version-ok", "#!/bin/sh\necho v9.9.9\n", "");

    let output = command_version_output(script.to_str().unwrap())
        .await
        .expect("script should run");
    assert_eq!(output.trim(), "v9.9.9");
}

#[tokio::test]
async fn command_version_output_reports_failure() {
    #[cfg(windows)]
    let script = temp_script(
        "version-fail",
        "@echo off\necho error>&2\nexit /b 1\n",
        ".cmd",
    );
    #[cfg(not(windows))]
    let script = temp_script("version-fail", "#!/bin/sh\necho error 1>&2\nexit 1\n", "");

    let err = command_version_output(script.to_str().unwrap())
        .await
        .expect_err("failing script should error");
    assert!(format!("{}", err).contains("返回非 0"));
}
