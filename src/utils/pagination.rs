//! 通用分页工具模块。
//!
//! 提供分页查询参数、分页响应结构以及内存分页 trait（`MemoryPaginate`）。

use serde::{Deserialize, Deserializer, Serialize};

/// 自定义反序列化：将字符串或数字转换为 Option<i64>
fn deserialize_optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(i64),
    }

    match Option::<StringOrInt>::deserialize(deserializer)? {
        None => Ok(None),
        Some(StringOrInt::Int(i)) => Ok(Some(i)),
        Some(StringOrInt::String(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<i64>().map(Some).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// 公共的分页查询参数，供 API 层复用
#[derive(Deserialize, Clone, Debug)]
pub struct PageQuery {
    /// 页码，从 1 开始，可选，默认为 1
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub page: Option<i64>,
    /// 每页数量，可选
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub page_size: Option<i64>,
}

impl PageQuery {
    /// 规范化分页参数，兜底默认页大小
    pub fn normalize(&self, default_page_size: i64) -> (i64, i64) {
        let page = self.page.unwrap_or(1).max(1);
        let page_size = self.page_size.unwrap_or(default_page_size).max(1);
        (page, page_size)
    }

    /// 使用默认页大小 10 的规范化逻辑
    pub fn normalize_default(&self) -> (i64, i64) {
        self.normalize(10)
    }
}

/// 统一的分页响应结构
#[derive(Serialize, Clone, Debug)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

impl<T> PageResponse<T> {
    /// 从数据库分页结果创建响应
    pub fn from_db(items: Vec<T>, total: i64, page: i64, page_size: i64) -> Self {
        Self {
            items,
            total,
            page,
            page_size,
        }
    }
}

/// 为 Vec 提供内存分页扩展（适用于一次性查出后在内存中分页的场景）
pub trait MemoryPaginate<T> {
    fn paginate(self, page: i64, page_size: i64) -> PageResponse<T>;
}

impl<T> MemoryPaginate<T> for Vec<T> {
    fn paginate(self, page: i64, page_size: i64) -> PageResponse<T> {
        let total = self.len() as i64;
        let page = page.max(1);
        let page_size = page_size.max(1);
        let start = ((page - 1) * page_size) as usize;

        let items = if start < self.len() {
            self.into_iter()
                .skip(start)
                .take(page_size as usize)
                .collect()
        } else {
            Vec::new()
        };

        PageResponse {
            items,
            total,
            page,
            page_size,
        }
    }
}
