use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "src/utils/tree_sitter_sync_manifest.rs"]
mod tree_sitter_sync_manifest;

use tree_sitter_sync_manifest::{TREE_SITTER_ASSET_SOURCES, TreeSitterAssetSource};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct EditorAssetManifest {
    language_id: String,
    parser_wasm: String,
    highlights_query: String,
    completion_bundle: Option<String>,
}

/// 读取当前工作区的 Cargo 元数据。
fn get_cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .expect("Failed to run cargo metadata");
    serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata JSON")
}

/// 从 Cargo 元数据中提取指定包的版本号。
fn get_package_version<'a>(packages: &'a [Value], name: &str) -> &'a str {
    packages
        .iter()
        .find(|pkg| pkg.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|pkg| pkg.get("version").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
}

/// 从 Cargo 元数据中定位指定包的根目录。
fn get_package_root(packages: &[Value], name: &str) -> Option<PathBuf> {
    packages
        .iter()
        .find(|pkg| pkg.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|pkg| pkg.get("manifest_path").and_then(|v| v.as_str()))
        .and_then(|manifest_path| Path::new(manifest_path).parent().map(Path::to_path_buf))
}

/// 将外部命令的标准输出和错误输出转成 Cargo warning。
fn print_command_output(label: &str, output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        println!("cargo:warning=[{} stdout] {}", label, line);
    }

    for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
        println!("cargo:warning=[{} stderr] {}", label, line);
    }
}

/// 确保目标文件的父目录存在。
fn ensure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("Failed to create asset parent directory");
    }
}

/// 按相同相对路径复制一份 tree-sitter 资产。
fn copy_asset(src_root: &Path, relative_path: &str, dest_root: &Path) {
    copy_asset_to(src_root, relative_path, dest_root, relative_path);
}

/// 将单个 tree-sitter 资产复制到指定目标相对路径。
fn copy_asset_to(src_root: &Path, relative_path: &str, dest_root: &Path, dest_relative_path: &str) {
    let src_path = src_root.join(relative_path);
    if !src_path.exists() {
        println!(
            "cargo:warning=语言资产不存在，跳过复制: {}",
            src_path.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed={}", src_path.display());

    let dest_path = dest_root.join(dest_relative_path);
    ensure_parent_dir(&dest_path);
    fs::copy(&src_path, &dest_path).unwrap_or_else(|err| {
        panic!(
            "Failed to copy asset from {} to {}: {}",
            src_path.display(),
            dest_path.display(),
            err
        )
    });
}

/// 读取语言清单，解析出 wasm、高亮和补全资源路径。
fn read_editor_asset_manifest(
    crate_root: &Path,
    source: &TreeSitterAssetSource,
) -> Result<EditorAssetManifest, String> {
    let manifest_path = crate_root.join(source.manifest_relative);
    let content = fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "读取语言清单失败: path={}, error={}",
            manifest_path.display(),
            err
        )
    })?;
    serde_json::from_str::<EditorAssetManifest>(&content).map_err(|err| {
        format!(
            "解析语言清单失败: path={}, error={}",
            manifest_path.display(),
            err
        )
    })
}

/// 判断一个语言资产根目录是否至少具备可复制的清单。
fn is_asset_root_usable(
    crate_root: &Path,
    source: &TreeSitterAssetSource,
) -> Result<EditorAssetManifest, String> {
    read_editor_asset_manifest(crate_root, source)
}

/// 使用 Cargo 已解析到的依赖目录。
fn resolve_package_asset_root(
    source: &TreeSitterAssetSource,
    package_root: Option<PathBuf>,
) -> Option<(PathBuf, EditorAssetManifest)> {
    let root = package_root?;
    match is_asset_root_usable(&root, source) {
        Ok(manifest) => Some((root, manifest)),
        Err(err) => {
            println!(
                "cargo:warning=Cargo 依赖 tree-sitter 资产不可用: package={}, manifest={}, error={}",
                source.package_name, source.manifest_relative, err
            );
            None
        }
    }
}

/// 注册 tree-sitter 语言源目录的变更监听，保证资源变更时重新执行 build.rs。
fn register_tree_sitter_inputs(crate_root: &Path) {
    for relative in ["editor", "queries", "completions"] {
        let path = crate_root.join(relative);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// 将 tree-sitter 语言资产复制到 `web/public/tree-sitter`。
///
/// 约束：
/// - 不在 build 阶段拉取远端仓库；
/// - 语言依赖由 Cargo 在解析 `Cargo.toml` 时准备；
/// - 这里只从 Cargo.lock 锁定且 Cargo 已解析到的依赖目录复制资源。
fn export_tree_sitter_assets(metadata: &Value) {
    let packages = metadata
        .get("packages")
        .and_then(|v| v.as_array())
        .expect("No packages found in cargo metadata");

    let public_root = Path::new("web/public/tree-sitter");
    let languages_root = public_root.join("languages");
    fs::create_dir_all(&languages_root).expect("Failed to create tree-sitter public directory");
    let mut exported_manifests = Vec::new();
    for source in TREE_SITTER_ASSET_SOURCES {
        let package_root = get_package_root(packages, source.package_name);

        if let Some((crate_root, manifest)) = resolve_package_asset_root(source, package_root) {
            register_tree_sitter_inputs(&crate_root);
            println!(
                "cargo:rerun-if-changed={}",
                crate_root.join(source.manifest_relative).display()
            );

            let language_root = languages_root.join(&manifest.language_id);
            copy_asset_to(
                &crate_root,
                source.manifest_relative,
                &language_root,
                "editor/asset-manifest.json",
            );
            copy_asset(&crate_root, &manifest.highlights_query, &language_root);
            copy_asset(&crate_root, &manifest.parser_wasm, &language_root);

            if let Some(bundle) = manifest.completion_bundle.as_deref() {
                copy_asset(&crate_root, bundle, &language_root);
            }

            exported_manifests.push(manifest);
            continue;
        }

        println!(
            "cargo:warning=未找到 tree-sitter 依赖目录，跳过语言资产导出: {}",
            source.package_name
        );
    }

    let index_path = languages_root.join("index.json");
    ensure_parent_dir(&index_path);
    fs::write(
        &index_path,
        serde_json::to_string_pretty(&exported_manifests)
            .expect("Failed to serialize tree-sitter manifest index"),
    )
    .expect("Failed to write tree-sitter manifest index");
}

/// 导出 `web-tree-sitter` 运行时 wasm 到前端静态目录。
fn export_web_tree_sitter_runtime() {
    let runtime_root = Path::new("web/public/tree-sitter");
    fs::create_dir_all(runtime_root).expect("Failed to create tree-sitter runtime directory");

    let runtime_src = Path::new("web/node_modules/web-tree-sitter/web-tree-sitter.wasm");
    if !runtime_src.exists() {
        println!(
            "cargo:warning=未找到 web-tree-sitter 运行时 wasm，跳过复制: {}",
            runtime_src.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed={}", runtime_src.display());

    let runtime_dest = runtime_root.join("tree-sitter.wasm");
    fs::copy(runtime_src, &runtime_dest).unwrap_or_else(|err| {
        panic!(
            "Failed to copy runtime wasm from {} to {}: {}",
            runtime_src.display(),
            runtime_dest.display(),
            err
        )
    });
}

/// 确保前端依赖和 tree-sitter 资源在开发构建前已就绪。
fn ensure_frontend_tree_sitter_assets(metadata: &Value) {
    println!("cargo:rerun-if-changed=web/package.json");
    println!("cargo:rerun-if-changed=web/package-lock.json");
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/vite.config.js");

    let npm_check = Command::new("npm").arg("--version").output();

    if npm_check.is_err() {
        println!("cargo:warning=未检测到 npm，跳过前端构建");
        return;
    }

    let runtime_src = Path::new("web/node_modules/web-tree-sitter/web-tree-sitter.wasm");
    if !runtime_src.exists() {
        let install_result = Command::new("npm")
            .arg("install")
            .current_dir("web")
            .output();

        match install_result {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=npm install 失败");
                    print_command_output("npm install", &output);
                    println!(
                        "cargo:warning=npm install 失败，退出码: {:?}",
                        output.status.code()
                    );
                    return;
                }
            }
            Err(e) => {
                println!("cargo:warning=npm install 失败: {}", e);
                return;
            }
        }
    }

    export_tree_sitter_assets(metadata);
    export_web_tree_sitter_runtime();
}

/// 执行前端构建，失败时仅输出 warning，不中断后端编译。
fn run_npm_build() {
    let build_result = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir("web")
        .output();

    match build_result {
        Ok(output) => {
            if !output.status.success() {
                println!("cargo:warning=前端构建失败");
                print_command_output("npm run build", &output);
                println!(
                    "cargo:warning=前端构建失败，退出码: {:?}",
                    output.status.code()
                );
            }
        }
        Err(e) => {
            println!("cargo:warning=npm run build 失败: {}", e);
        }
    }
}

/// build.rs 入口：开发态同步前端资源并写入版本环境变量。
fn main() {
    let is_release = std::env::var("PROFILE").unwrap_or_default() == "release";

    let metadata = get_cargo_metadata();
    if is_release {
        println!("cargo:warning=Release 构建，跳过前端资源同步与 npm 构建");
    } else {
        ensure_frontend_tree_sitter_assets(&metadata);
        run_npm_build();
    }

    let app_name = env!("CARGO_PKG_NAME");
    let wp_parse_pkg_name = "wp-engine";
    let wfusion_pkg_name = "wf-engine";

    let packages = metadata
        .get("packages")
        .and_then(|v| v.as_array())
        .expect("No packages found in cargo metadata");

    let wp_station = get_package_version(packages, app_name);
    let wp_parse = get_package_version(packages, wp_parse_pkg_name);
    let wfusion = get_package_version(packages, wfusion_pkg_name);

    println!("cargo:rustc-env=WP_STATION_VERSION={}", wp_station);
    println!("cargo:rustc-env=WP_PARSE_VERSION={}", wp_parse);
    println!("cargo:rustc-env=WFUSION_VERSION={}", wfusion);
}
