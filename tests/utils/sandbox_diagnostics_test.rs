use std::fs;
use std::path::PathBuf;

use wp_station::server::Setting;
use wp_station::server::sandbox::SandboxStage;
use wp_station::server::sandbox_diagnostics::collect_stage_hits;

fn write_log(path: &PathBuf, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn collect_stage_hits_reads_stage_log() {
    let log = std::env::temp_dir().join(format!(
        "sandbox-diag-{}.log",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_log(
        &log,
        "[DIAG] data/out_dat/miss.dat\n[DIAG] data/out_dat/error.dat\n",
    );

    let hits = collect_stage_hits(SandboxStage::AnalyseRuntimeOutput, log.to_str(), None);
    assert!(hits.len() >= 2);
    assert!(hits.iter().any(|hit| hit.suggestion.contains("miss.dat")));
    assert!(hits.iter().any(|hit| hit.suggestion.contains("error.dat")));

    fs::remove_file(log).unwrap();
}

#[test]
fn collect_stage_hits_supports_relative_paths() {
    let workspace = Setting::workspace_root().clone();
    let relative = PathBuf::from("tmp/sandbox-diag-relative.log");
    let absolute = workspace.join(&relative);
    write_log(&absolute, "[DIAG] data/out_dat/default.dat\n");

    let hits = collect_stage_hits(SandboxStage::AnalyseRuntimeOutput, relative.to_str(), None);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].suggestion.contains("default.dat"));

    fs::remove_file(absolute).unwrap();
}
