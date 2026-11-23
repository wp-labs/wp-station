//! 根据本次导入源的专属配置和规则结构识别系统，避免交叉导入。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::utils::SystemKind;

/// 在合成现有项目之前检查源目录，避免已有文件掩盖导入包的系统类型。
pub(super) fn validate_import_system(
    source_dir: &Path,
    target: SystemKind,
) -> Result<(), AppError> {
    let mut wparse = Vec::new();
    let mut wfusion = Vec::new();

    for (file, evidence) in [
        ("conf/wparse.toml", &mut wparse),
        ("conf/wfusion.toml", &mut wfusion),
    ] {
        if source_dir.join(file).is_file() {
            evidence.push(file.to_string());
        }
    }

    if source_dir.join("models/windows.toml").is_file() {
        wfusion.push("models/windows.toml".to_string());
    }
    collect_model_evidence(source_dir, "wpl", "wpl", &mut wparse)?;
    collect_model_evidence(source_dir, "oml", "oml", &mut wparse)?;
    collect_model_evidence(source_dir, "rules", "wfl", &mut wfusion)?;
    collect_model_evidence(source_dir, "schemas", "wfs", &mut wfusion)?;
    collect_model_evidence(source_dir, "scenarios", "wfg", &mut wfusion)?;

    if !wparse.is_empty() && !wfusion.is_empty() {
        return Err(AppError::validation(format!(
            "导入内容混用了 WParse 与 WFusion 配置或规则：WParse 示例：{}；WFusion 示例：{}。请分别导入对应系统",
            wparse[0], wfusion[0]
        )));
    }

    let (other_name, other_evidence, target_name) = match target {
        SystemKind::Wparse => ("WFusion", &wfusion, "WParse"),
        SystemKind::Wfusion => ("WParse", &wparse, "WFusion"),
    };
    if let Some(path) = other_evidence.first() {
        return Err(AppError::validation(format!(
            "所选项目包含 {other_name} 配置或规则（{path}），当前选择的是 {target_name}。请切换系统后重新导入"
        )));
    }
    Ok(())
}

fn collect_model_evidence(
    source_dir: &Path,
    group: &str,
    extension: &str,
    evidence: &mut Vec<String>,
) -> Result<(), AppError> {
    let root = source_dir.join("models").join(group);
    if !root.is_dir() {
        return Ok(());
    }
    let mut pending = vec![root];
    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(&dir)
            .map_err(AppError::internal)?
            .map(|entry| entry.map(|entry| entry.path()).map_err(AppError::internal))
            .collect::<Result<Vec<PathBuf>, AppError>>()?;
        entries.sort();
        for path in entries {
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() && path.extension().is_some_and(|ext| ext == extension) {
                evidence.push(
                    path.strip_prefix(source_dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
                return Ok(());
            }
        }
    }
    Ok(())
}
