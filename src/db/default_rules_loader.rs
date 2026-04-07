use crate::error::DbResult;
use chrono::Utc;
use rust_embed::RustEmbed;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "default_configs/"]
struct DefaultConfigs;

/// 从嵌入的默认配置目录初始化数据库
pub async fn init_default_configs_from_embedded(db: &DatabaseConnection) -> DbResult<()> {
    info!("开始从嵌入的默认配置初始化数据库");

    // 初始化规则配置
    init_rule_configs_from_embedded(db).await?;

    // 初始化知识库配置
    init_knowledge_configs_from_embedded(db).await?;

    info!("默认配置初始化完成");
    Ok(())
}

/// 从嵌入资源初始化规则配置
async fn init_rule_configs_from_embedded(db: &DatabaseConnection) -> DbResult<()> {
    let backend = db.get_database_backend();
    let now = Utc::now();
    let now_str = now.naive_utc().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

    // 遍历所有嵌入的文件
    for file_path in DefaultConfigs::iter() {
        let path_str = file_path.as_ref();

        // 跳过 README 和知识库目录
        if path_str.contains("README") || path_str.starts_with("models/knowledge/") {
            continue;
        }

        // 解析文件类型和路径
        if let Some((rule_type, file_name)) = parse_rule_type_and_name(path_str)
            && let Some(content_file) = DefaultConfigs::get(path_str)
        {
            let content = std::str::from_utf8(content_file.data.as_ref())
                .map_err(|e| sea_orm::DbErr::Custom(format!("UTF-8 解析失败: {}", e)))?;

            let file_size = content.len() as i32;
            let display_name = default_display_name(rule_type, &file_name);

            let sample_content = if rule_type == "wpl" {
                let sample_path = format!("models/wpl/{}/sample.dat", file_name);
                DefaultConfigs::get(&sample_path).and_then(|sample_file| {
                    std::str::from_utf8(sample_file.data.as_ref())
                        .ok()
                        .map(|s| s.to_string())
                })
            } else {
                None
            };
            let sample_content_sql = sql_value_or_null(sample_content.as_deref());
            let content_escaped = content.replace("'", "''");

            // 插入到数据库
            let sql = format!(
                r#"INSERT INTO public.rule_configs ("type", file_name, display_name, "content", sample_content, file_size, updated_at, created_at, "version", is_active)
                VALUES ('{}', '{}', {}, '{}', {}, {}, '{}', '{}', 1, true)
                ON CONFLICT DO NOTHING;"#,
                rule_type,
                file_name,
                sql_value_or_null(display_name.as_deref()),
                content_escaped,
                sample_content_sql,
                file_size,
                now_str,
                now_str
            );

            db.execute(Statement::from_string(backend, sql)).await?;
            debug!("导入规则配置: type={}, file={}", rule_type, file_name);
        }
    }

    Ok(())
}

/// 从嵌入资源初始化知识库配置
async fn init_knowledge_configs_from_embedded(db: &DatabaseConnection) -> DbResult<()> {
    let backend = db.get_database_backend();
    let now = Utc::now();
    let now_str = now.naive_utc().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

    // 查找所有知识库目录
    let knowledge_dirs: std::collections::HashSet<String> = DefaultConfigs::iter()
        .filter_map(|path| {
            let path_str = path.as_ref();
            if path_str.starts_with("models/knowledge/") && !path_str.ends_with("knowdb.toml") {
                // 提取目录名: models/knowledge/example_score/xxx -> example_score
                let parts: Vec<&str> = path_str.split('/').collect();
                if parts.len() >= 3 {
                    return Some(parts[2].to_string());
                }
            }
            None
        })
        .collect();

    for table_name in knowledge_dirs {
        let config_path = "models/knowledge/knowdb.toml".to_string();
        let create_sql_path = format!("models/knowledge/{}/create.sql", table_name);
        let insert_sql_path = format!("models/knowledge/{}/insert.sql", table_name);
        let data_csv_path = format!("models/knowledge/{}/data.csv", table_name);

        // 读取配置内容 - 先转换为String再返回
        let config_content = DefaultConfigs::get(&config_path).and_then(|f| {
            std::str::from_utf8(f.data.as_ref())
                .ok()
                .map(|s| s.to_string())
        });

        let create_sql = DefaultConfigs::get(&create_sql_path).and_then(|f| {
            std::str::from_utf8(f.data.as_ref())
                .ok()
                .map(|s| s.to_string())
        });

        let insert_sql = DefaultConfigs::get(&insert_sql_path).and_then(|f| {
            std::str::from_utf8(f.data.as_ref())
                .ok()
                .map(|s| s.to_string())
        });

        let data_content = DefaultConfigs::get(&data_csv_path).and_then(|f| {
            std::str::from_utf8(f.data.as_ref())
                .ok()
                .map(|s| s.to_string())
        });

        // 如果至少有一个文件存在,则插入
        if config_content.is_some()
            || create_sql.is_some()
            || insert_sql.is_some()
            || data_content.is_some()
        {
            let sql = format!(
                r#"INSERT INTO public.knowledge_configs (file_name, config_content, create_sql, insert_sql, data_content, is_active, updated_at, created_at)
                VALUES ('{}', {}, {}, {}, {}, true, '{}', '{}')
                ON CONFLICT DO NOTHING;"#,
                table_name,
                sql_value_or_null(config_content.as_deref()),
                sql_value_or_null(create_sql.as_deref()),
                sql_value_or_null(insert_sql.as_deref()),
                sql_value_or_null(data_content.as_deref()),
                now_str,
                now_str
            );

            db.execute(Statement::from_string(backend, sql)).await?;
            debug!("导入知识库配置: table={}", table_name);
        }
    }

    Ok(())
}

/// 解析文件路径,返回 (rule_type, file_name)
fn parse_rule_type_and_name(path: &str) -> Option<(&'static str, String)> {
    let path_obj = Path::new(path);

    // conf/wparse.toml -> (parse, wparse.toml)
    if path.starts_with("conf/wparse.toml") {
        return Some(("parse", "wparse.toml".to_string()));
    }

    // conf/wpgen.toml -> (wpgen, wpgen.toml)
    if path.starts_with("conf/wpgen.toml") {
        return Some(("wpgen", "wpgen.toml".to_string()));
    }

    // connectors/sink.d/xxx.toml -> (sink_connect, xxx.toml)
    if path.starts_with("connectors/sink.d/") {
        let file_name = path_obj.file_name()?.to_str()?.to_string();
        return Some(("sink_connect", file_name));
    }

    // connectors/source.d/xxx.toml -> (source_connect, xxx.toml)
    if path.starts_with("connectors/source.d/") {
        let file_name = path_obj.file_name()?.to_str()?.to_string();
        return Some(("source_connect", file_name));
    }

    // topology/sources/xxx -> (source, xxx)
    if path.starts_with("topology/sources/") {
        let relative = path.strip_prefix("topology/sources/")?;
        return Some(("source", relative.to_string()));
    }

    // topology/sinks/xxx -> (sink, xxx)
    if path.starts_with("topology/sinks/") {
        let relative = path.strip_prefix("topology/sinks/")?;
        return Some(("sink", relative.to_string()));
    }

    // models/wpl/xxx/parse.wpl -> (wpl, xxx)
    if path.starts_with("models/wpl/") && path.ends_with("/parse.wpl") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 {
            return Some(("wpl", parts[2].to_string()));
        }
    }

    // models/oml/xxx/adm.oml -> (oml, xxx)
    if path.starts_with("models/oml/") && path.ends_with("/adm.oml") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 3 {
            return Some(("oml", parts[2].to_string()));
        }
    }

    None
}

fn default_display_name(rule_type: &str, file_name: &str) -> Option<String> {
    match (rule_type, file_name) {
        ("source_connect", "00-file-default.toml") => Some("文件".to_string()),
        ("source_connect", "10-syslog-udp.toml") => Some("syslog(UDP)".to_string()),
        ("source_connect", "11-syslog-tcp.toml") => Some("syslog(TCP)".to_string()),
        ("source_connect", "12-tcp.toml") => Some("TCP".to_string()),
        ("source_connect", "30-kafka.toml") => Some("Kafka".to_string()),
        ("sink_connect", "10-syslog-udp.toml") => Some("syslog(UDP)".to_string()),
        ("sink_connect", "11-syslog-tcp.toml") => Some("syslog(TCP)".to_string()),
        ("sink_connect", "12-tcp.toml") => Some("TCP".to_string()),
        ("sink_connect", "30-kafka.toml") => Some("Kafka".to_string()),
        ("sink_connect", "40-prometheus.toml") => Some("Prometheus".to_string()),
        ("sink_connect", "00-blackhole-sink.toml") => Some("Blackhole".to_string()),
        ("sink_connect", "01-file-prototext.toml") => Some("文件(Prototext)".to_string()),
        ("sink_connect", "02-file-json.toml") => Some("文件(JSON)".to_string()),
        ("sink_connect", "03-file-kv.toml") => Some("文件(KV)".to_string()),
        ("sink_connect", "04-file-raw.toml") => Some("文件(RAW)".to_string()),
        _ => None,
    }
}

/// 将字符串转换为 SQL 值或 NULL
fn sql_value_or_null(value: Option<&str>) -> String {
    match value {
        Some(s) => format!("'{}'", s.replace("'", "''")),
        None => "NULL".to_string(),
    }
}
