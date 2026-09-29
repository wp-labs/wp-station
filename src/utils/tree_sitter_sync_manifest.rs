/// Tree-sitter 语言资产远程源定义。
///
/// 这份定义同时被 `build.rs` 和运行时开发态同步逻辑复用，
/// 避免两边各自维护一套本地覆盖目录和语言清单路径。
#[derive(Clone, Copy, Debug)]
pub(crate) struct TreeSitterAssetSource {
    pub package_name: &'static str,
    pub manifest_relative: &'static str,
}

pub(crate) const TREE_SITTER_ASSET_SOURCES: &[TreeSitterAssetSource] = &[
    TreeSitterAssetSource {
        package_name: "tree-sitter-wpl",
        manifest_relative: "editor/asset-manifest.json",
    },
    TreeSitterAssetSource {
        package_name: "tree-sitter-oml",
        manifest_relative: "editor/asset-manifest.json",
    },
    TreeSitterAssetSource {
        package_name: "tree-sitter-wfl",
        manifest_relative: "editor/wfs/asset-manifest.json",
    },
    TreeSitterAssetSource {
        package_name: "tree-sitter-wfl",
        manifest_relative: "editor/wfl/asset-manifest.json",
    },
    TreeSitterAssetSource {
        package_name: "tree-sitter-wfl",
        manifest_relative: "editor/wfg/asset-manifest.json",
    },
];
