//! Tree-sitter 运行时资源辅助。
//!
//! - `/tree-sitter/*` 资源优先从 `web/public` 直接读取；
//! - 开发态启动时会尝试从远程仓库同步最新语言资产到 `web/public/tree-sitter`；
//! - 同步失败时回退到现有本地资产，不阻断服务启动。

use super::tree_sitter_sync_manifest::{TREE_SITTER_ASSET_SOURCES, TreeSitterAssetSource};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Debug, Deserialize, Serialize, Clone)]
struct EditorAssetManifest {
    language_id: String,
    parser_wasm: String,
    highlights_query: String,
    completion_bundle: Option<String>,
}

fn repo_root() -> PathBuf {
    PathBuf::from(MANIFEST_DIR)
}

fn tree_sitter_public_root() -> PathBuf {
    repo_root().join("web/public/tree-sitter")
}

fn tree_sitter_languages_root() -> PathBuf {
    tree_sitter_public_root().join("languages")
}

fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn copy_asset_to(
    src_root: &Path,
    relative_path: &str,
    dest_root: &Path,
    dest_relative_path: &str,
) -> std::io::Result<()> {
    let src_path = src_root.join(relative_path);
    if !src_path.exists() {
        return Ok(());
    }

    let dest_path = dest_root.join(dest_relative_path);
    ensure_parent_dir(&dest_path)?;
    fs::copy(src_path, dest_path)?;
    Ok(())
}

fn copy_asset(src_root: &Path, relative_path: &str, dest_root: &Path) -> std::io::Result<()> {
    copy_asset_to(src_root, relative_path, dest_root, relative_path)
}

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

fn is_asset_root_usable(
    crate_root: &Path,
    source: &TreeSitterAssetSource,
) -> Result<EditorAssetManifest, String> {
    read_editor_asset_manifest(crate_root, source)
}

fn get_cargo_metadata() -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(repo_root())
        .output()
        .map_err(|err| format!("执行 cargo metadata 失败: {}", err))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "cargo metadata 失败: status={:?}, stdout={}, stderr={}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("解析 cargo metadata 输出失败: {}", err))
}

fn get_package_root(packages: &[serde_json::Value], name: &str) -> Option<PathBuf> {
    packages
        .iter()
        .find(|pkg| pkg.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|pkg| pkg.get("manifest_path").and_then(|v| v.as_str()))
        .and_then(|manifest_path| Path::new(manifest_path).parent().map(Path::to_path_buf))
}

fn resolve_package_asset_root(
    source: &TreeSitterAssetSource,
    package_root: Option<PathBuf>,
) -> Option<(PathBuf, EditorAssetManifest)> {
    let root = package_root?;
    match is_asset_root_usable(&root, source) {
        Ok(manifest) => Some((root, manifest)),
        Err(err) => {
            tracing::warn!(
                "Cargo 依赖 tree-sitter 资产不可用: package={}, manifest={}, error={}",
                source.package_name,
                source.manifest_relative,
                err
            );
            None
        }
    }
}

fn export_language_assets(
    crate_root: &Path,
    manifest: &EditorAssetManifest,
    source: &TreeSitterAssetSource,
) -> Result<EditorAssetManifest, String> {
    let language_root = tree_sitter_languages_root().join(&manifest.language_id);
    copy_asset_to(
        crate_root,
        source.manifest_relative,
        &language_root,
        "editor/asset-manifest.json",
    )
    .map_err(|err| {
        format!(
            "复制语言清单失败: language={}, error={}",
            manifest.language_id, err
        )
    })?;
    copy_asset(crate_root, &manifest.highlights_query, &language_root).map_err(|err| {
        format!(
            "复制高亮查询失败: language={}, error={}",
            manifest.language_id, err
        )
    })?;
    copy_asset(crate_root, &manifest.parser_wasm, &language_root).map_err(|err| {
        format!(
            "复制 parser wasm 失败: language={}, error={}",
            manifest.language_id, err
        )
    })?;

    if let Some(bundle) = manifest.completion_bundle.as_deref() {
        copy_asset(crate_root, bundle, &language_root).map_err(|err| {
            format!(
                "复制补全包失败: language={}, error={}",
                manifest.language_id, err
            )
        })?;
    }

    Ok(manifest.clone())
}

fn export_tree_sitter_runtime() -> Result<(), String> {
    let runtime_src = repo_root().join("web/node_modules/web-tree-sitter/web-tree-sitter.wasm");
    if !runtime_src.exists() {
        return Ok(());
    }

    let runtime_dest = tree_sitter_public_root().join("tree-sitter.wasm");
    ensure_parent_dir(&runtime_dest)
        .map_err(|err| format!("创建 runtime wasm 目录失败: {}", err))?;
    fs::copy(&runtime_src, &runtime_dest).map_err(|err| {
        format!(
            "复制 runtime wasm 失败: from={}, to={}, error={}",
            runtime_src.display(),
            runtime_dest.display(),
            err
        )
    })?;
    Ok(())
}

/// 开发态服务启动时同步 tree-sitter 语言资产。
///
/// 说明：
/// - `build.rs` 只能在 Cargo 判定需要重新构建时运行，无法保证每次 `cargo run`
///   都会执行；
/// - 因此这里在开发态启动阶段补一次同步，确保 `/tree-sitter/*` 总能尽量拿到远程最新资产。
pub fn sync_tree_sitter_assets_for_dev_start() -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }

    fs::create_dir_all(tree_sitter_languages_root())
        .map_err(|err| format!("创建 tree-sitter 语言目录失败: {}", err))?;

    let mut exported_manifests = Vec::new();
    let metadata = get_cargo_metadata()?;
    let packages = metadata
        .get("packages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "cargo metadata 缺少 packages".to_string())?;

    for source in TREE_SITTER_ASSET_SOURCES {
        let package_root = get_package_root(packages, source.package_name);

        let Some((crate_root, manifest)) = resolve_package_asset_root(source, package_root) else {
            tracing::warn!(
                "未找到可用的 tree-sitter 资产目录，跳过导出: package={}",
                source.package_name
            );
            continue;
        };

        match export_language_assets(&crate_root, &manifest, source) {
            Ok(manifest) => exported_manifests.push(manifest),
            Err(err) => {
                tracing::warn!(
                    "导出 tree-sitter 语言资产失败: package={}, error={}",
                    source.package_name,
                    err
                );
            }
        }
    }

    if let Err(err) = export_tree_sitter_runtime() {
        tracing::warn!("导出 tree-sitter runtime wasm 失败: error={}", err);
    }

    let index_path = tree_sitter_languages_root().join("index.json");
    ensure_parent_dir(&index_path).map_err(|err| format!("创建语言索引目录失败: {}", err))?;
    fs::write(
        &index_path,
        serde_json::to_string_pretty(&exported_manifests)
            .map_err(|err| format!("序列化语言索引失败: {}", err))?,
    )
    .map_err(|err| format!("写入语言索引失败: {}", err))?;

    Ok(())
}

/// 尝试从本地磁盘读取 `/tree-sitter/*` 资源。
pub fn read_runtime_asset_from_public(request_path: &str) -> std::io::Result<Option<Vec<u8>>> {
    let relative_path = request_path.trim_start_matches('/');
    if !relative_path.starts_with("tree-sitter/") {
        return Ok(None);
    }

    let asset_path = repo_root().join("web/public").join(relative_path);
    if !asset_path.exists() {
        return Ok(None);
    }

    Ok(Some(fs::read(asset_path)?))
}
