use std::fs;
use std::path::PathBuf;

use wp_station::utils::process_guard::{check_command_exists, command_version_output};

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

#[test]
fn check_command_exists_accepts_absolute_path() {
    let exe = std::env::current_exe().unwrap();
    check_command_exists(exe.to_str().unwrap()).expect("current executable exists");
}

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
