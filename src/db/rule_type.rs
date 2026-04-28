use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

/// 规则类型 / 连接配置类型：wpl / oml / knowledge / source / sink / parse / wpgen / source_connect / sink_connect 等
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, Display, EnumString, AsRefStr, PartialEq, Eq, Hash,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RuleType {
    All,
    Wpl,
    Oml,
    Knowledge,
    Source,
    Sink,
    Parse,
    Wpgen,
    #[serde(rename = "source_connect")]
    SourceConnect,
    #[serde(rename = "sink_connect")]
    SinkConnect,
}

impl RuleType {
    /// 映射到项目校验组件
    pub fn to_check_component(&self) -> Vec<wp_proj::project::checker::CheckComponent> {
        use wp_proj::project::checker::CheckComponent;
        match self {
            RuleType::All => vec![
                CheckComponent::Wpl,
                CheckComponent::Oml,
                CheckComponent::Engine,
                CheckComponent::Sources,
                CheckComponent::Sinks,
                CheckComponent::Connectors,
                CheckComponent::Wpgen,
                CheckComponent::SemanticDict,
            ],
            RuleType::Wpl => vec![CheckComponent::Wpl],
            RuleType::Oml => vec![CheckComponent::Oml],
            RuleType::Knowledge => vec![CheckComponent::Engine], // todo 缺少知识库校验
            RuleType::Source => vec![CheckComponent::Sources],
            RuleType::Sink => vec![CheckComponent::Sinks],
            RuleType::Parse => vec![CheckComponent::Engine],
            RuleType::Wpgen => vec![CheckComponent::Engine],
            RuleType::SourceConnect | RuleType::SinkConnect => vec![CheckComponent::Connectors],
        }
    }
}
