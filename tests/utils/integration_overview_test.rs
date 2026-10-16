use std::fs;

use wp_station::server::RepoLayout;
use wp_station::utils::{
    SystemKind, load_integration_rule_overview_from_layout,
    load_integration_runtime_overview_from_layout,
};

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

#[test]
fn test_load_integration_runtime_overview_extracts_source_and_sink_details() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let layout = RepoLayout {
        models_root: temp_dir.path().join("project_models"),
        infra_root: temp_dir.path().join("project_infra"),
        connectors_root: temp_dir.path().join("shared_connectors"),
    };

    write_file(
        &layout
            .connectors_root
            .join("connectors/source.d/10-syslog-udp.toml"),
        r#"
id = "syslog_udp_src"
type = "syslog"
allow_override = ["addr", "port", "protocol", "header_mode"]

[connectors.params]
addr = "0.0.0.0"
port = 514
protocol = "udp"
header_mode = "strip"
"#,
    );
    write_file(
        &layout
            .connectors_root
            .join("connectors/sink.d/02-file-json.toml"),
        r#"
id = "file_json_sink"
type = "file"
allow_override = ["base", "file", "sync"]

[connectors.params]
base = "./data/out_dat"
file = "default.json"
sync = false
"#,
    );
    write_file(
        &layout.infra_root.join("topology/sources/wpsrc.toml"),
        r#"
[[sources]]
key = "gen_udp"
enable = true
connect = "syslog_udp_src"

[sources.params]
addr = "0.0.0.0"
port = 31601
protocol = "udp"
header_mode = "strip"
"#,
    );
    write_file(
        &layout
            .infra_root
            .join("topology/sinks/business.d/sink.toml"),
        r#"
version = "1.0"

[sink_group]
name = "all"
oml = ["*"]
parallel = 1

[[sink_group.sinks]]
name = "all_sink"
connect = "file_json_sink"
tags = []

[sink_group.sinks.params]
base = "./data/out_dat/"
file = "all.json"
"#,
    );

    let overview =
        load_integration_runtime_overview_from_layout(&layout).expect("load integration overview");

    assert_eq!(overview.supported_source_type_count, 1);
    assert_eq!(overview.supported_sink_type_count, 1);
    assert_eq!(overview.sources.len(), 1);
    assert_eq!(overview.sinks.len(), 1);

    let source = &overview.sources[0];
    assert_eq!(source.title, "gen_udp");
    assert_eq!(source.type_key, "syslog-udp");
    assert!(source.detail.contains("地址 0.0.0.0"));
    assert!(source.detail.contains("端口 31601"));
    assert!(source.detail.contains("协议 udp"));

    let sink = &overview.sinks[0];
    assert_eq!(sink.title, "sink.toml");
    assert_eq!(sink.type_key, "file");
    assert_eq!(sink.detail, "文件路径 ./data/out_dat/all.json");
}

#[test]
fn test_load_integration_rule_overview_extracts_device_and_log_types() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let layout = RepoLayout {
        models_root: temp_dir.path().join("project_models"),
        infra_root: temp_dir.path().join("project_infra"),
        connectors_root: temp_dir.path().join("shared_connectors"),
    };

    write_file(
        &layout.models_root.join("models/wpl/nginx/parse.wpl"),
        r#"
#[copy_raw(name:"raw_msg"), tag(dev_type: "Nginx设备", dev_name: "Nginx设备名称")]
package nginx {
  #[tag(log_desc: "访问日志")]
  rule access {
  }

  #[tag(log_desc: "访问日志")]
  rule access_detail {
  }

  #[tag(log_desc: "错误日志")]
  rule error {
  }

  rule ignore_login {
  }
}
"#,
    );
    write_file(
        &layout.models_root.join("models/wpl/ignore_pkg/parse.wpl"),
        r#"
package ignore_pkg {
  rule test {
  }
}
"#,
    );

    let overview = load_integration_rule_overview_from_layout(SystemKind::Wparse, &layout)
        .expect("load rule overview");

    assert_eq!(overview.items.len(), 1);
    let item = &overview.items[0];
    assert_eq!(item.key, "nginx");
    assert_eq!(item.device_type, "Nginx设备名称");
    assert_eq!(item.log_types.len(), 2);
    assert_eq!(item.log_types[0].log_type_name, "访问日志");
    assert_eq!(
        item.log_types[0].rule_keys,
        vec!["access".to_string(), "access_detail".to_string()]
    );
    assert_eq!(item.log_types[1].log_type_name, "错误日志");
    assert_eq!(item.log_types[1].rule_keys, vec!["error".to_string()]);
}

#[test]
fn test_load_wfusion_integration_rule_overview_extracts_wfs_and_wfl_counts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let layout = RepoLayout {
        models_root: temp_dir.path().join("project_models"),
        infra_root: temp_dir.path().join("project_infra"),
        connectors_root: temp_dir.path().join("shared_connectors"),
    };

    write_file(
        &layout
            .models_root
            .join("models/schemas/network/network.wfs"),
        "window network {}",
    );
    write_file(
        &layout.models_root.join("models/schemas/auth/auth.wfs"),
        "window auth {}\nrule auth_rule {}",
    );
    write_file(
        &layout
            .models_root
            .join("models/rules/ssh/ssh_brute_force.wfl"),
        "rule ssh_brute_force {}",
    );

    let overview = load_integration_rule_overview_from_layout(SystemKind::Wfusion, &layout)
        .expect("load wfusion rule overview");

    assert!(overview.items.is_empty());
    assert_eq!(overview.window_structure_count, 2);
    assert_eq!(overview.association_rule_count, 1);
    assert_eq!(
        overview
            .window_structures
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["auth.wfs", "network.wfs"]
    );
    assert_eq!(
        overview
            .association_rules
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["ssh_brute_force.wfl"]
    );
    assert_eq!(
        overview.window_structures[0].rule_names,
        vec!["auth_rule".to_string()]
    );
    assert_eq!(
        overview.association_rules[0].rule_names,
        vec!["ssh_brute_force".to_string()]
    );
}
