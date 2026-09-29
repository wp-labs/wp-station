//! OML 数据处理与格式化模块。
//!
//! 对外保留两类能力：
//! - 基于 OML 模型的 `DataRecord` 异步转换；
//! - OML 文本格式化（直接复用 tree-sitter-oml 提供的格式化器）。

mod transform;

pub use transform::convert_record;
pub use tree_sitter_oml::{OmlFormatError, OmlFormatter};
