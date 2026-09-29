//! WPL 解析与格式化模块。
//!
//! 对外保留三类能力：
//! - WPL 规则校验与日志解析；
//! - `DataRecord` 到字段列表的转换；
//! - WPL 文本格式化（直接复用 tree-sitter-wpl 提供的格式化器）。

mod parse;

pub use parse::{ParsedField, record_to_fields, warp_check_record};
pub use tree_sitter_wpl::{WplFormatError, WplFormatter};
