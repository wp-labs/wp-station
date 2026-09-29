//! OML `DataRecord` 转换逻辑。

use crate::error::AppError;
use wp_knowledge::cache::FieldQueryCache;
use wp_model_core::model::DataRecord;
use wp_oml::{AsyncDataTransformer, oml_parse_raw};

/// 使用 OML 模型转换单条 `DataRecord`。
pub async fn convert_record(oml: &str, record: DataRecord) -> Result<DataRecord, AppError> {
    let filter_oml = oml
        .lines()
        .map(|line| {
            if let Some(comment_start) = line.find("//") {
                &line[0..comment_start]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let model = oml_parse_raw(&mut filter_oml.as_str())
        .await
        .map_err(|e| AppError::oml_transform(format!("OML 语法解析错误: {:?}", e)))?;
    let mut cache = FieldQueryCache::with_capacity(10);
    Ok(model.transform_async(record, &mut cache).await)
}
