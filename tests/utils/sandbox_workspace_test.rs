use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use wp_station::server::Setting;
use wp_station::utils::constants::OUTPUT_PATHS;
use wp_station::utils::sandbox_workspace::{SandboxWorkspace, collect_output_checks};

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
        source_project_root: workspace_root.clone(),
    };

    let nested = workspace_root.join("foo/bar/example.txt");
    let display = ws.display_relative(&nested);

    assert!(display.contains("foo"));
    assert!(!display.starts_with('/'));
    assert!(!display.starts_with("\\"));
}

#[test]
fn bundle_logs_writes_sections() {
    let base = temp_dir("bundle-logs");
    let logs_dir = base.join("logs");
    fs::create_dir_all(&logs_dir).unwrap();
    let project_dir = base.join("project");
    fs::create_dir_all(&project_dir).unwrap();

    let workspace = SandboxWorkspace {
        root: base.clone(),
        project_dir: project_dir.clone(),
        logs_dir: logs_dir.clone(),
        source_project_root: base.join("source"),
    };

    let existing = project_dir.join("existing.log");
    if let Some(parent) = existing.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&existing, "line-a\nline-b\n").unwrap();
    let missing = project_dir.join("missing.log");

    let bundle_path = workspace
        .bundle_logs(
            "bundle.txt",
            &[("存在", existing.as_path()), ("缺失", missing.as_path())],
        )
        .expect("bundle logs");
    let content = fs::read_to_string(&bundle_path).unwrap();
    assert!(content.contains("===== 存在 ====="));
    assert!(content.contains("line-a"));
    assert!(content.contains("文件不存在"));

    fs::remove_dir_all(&base).unwrap();
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
        source_project_root: base.join("source"),
    };

    let listing = workspace
        .render_tree_listing(3, 10)
        .expect("render tree listing");
    assert!(listing.contains("dir_a"));
    assert!(listing.contains("file.txt"));

    fs::remove_dir_all(&base).unwrap();
}
