use wp_model_core::model::{DataField, DataRecord, DataType, Value};
use wp_station::utils::oml::{OmlFormatter, convert_record};

fn sample_record() -> DataRecord {
    DataRecord::from(vec![
        DataField::new(DataType::Chars, "http/agent", Value::from("agent")),
        DataField::new(DataType::Chars, "http/request", Value::from("GET /")),
    ])
}

#[test]
fn test_oml_formatter_normalizes_spacing() {
    let formatter = OmlFormatter::new();
    let formatted = formatter
        .format_content("name:test\nrule:/foo/*\n---\nout = read(src);")
        .expect("format simple oml");
    assert!(formatted.contains("name :"));
    assert!(formatted.contains("rule :"));
}

#[test]
fn test_convert_record_transforms_fields() {
    let oml_script = r#"name : sample
rule : /sample/*
---
http_agent = read(http/agent) ;
http_request = read(http/request) ;
"#;
    let record = sample_record();
    let converted = convert_record(oml_script, record).expect("convert record");
    let http_agent = converted
        .get_field("http_agent")
        .and_then(|field| field.get_value().as_str())
        .unwrap();
    assert_eq!(http_agent, "agent");
}

#[test]
fn test_convert_record_invalid_script_returns_error() {
    let record = sample_record();
    let bad_script = "invalid toml here";
    let err = convert_record(bad_script, record).expect_err("invalid OML should fail");
    assert!(format!("{:?}", err).contains("OML 语法解析错误"));
}

#[test]
fn test_oml_formatter_handles_comments_and_spacing() {
    let formatter = OmlFormatter::new();
    let messy = "name:demo\nrule:/demo/*\n// comment\n---\nvalue = read(raw) ;";
    let output = formatter
        .format_content(messy)
        .expect("format with comments");
    assert!(output.contains("value = read(raw);") || output.contains("value = read(raw)"));
    assert!(output.contains("name :"));
}

#[test]
fn test_oml_formatter_or_original_returns_input_on_error() {
    let formatter = OmlFormatter::new();
    let invalid = "name :: ???";
    let output = formatter.format_content_or_original(invalid);
    assert!(output.contains("name"));
}

#[test]
fn test_oml_formatter_formats_project_samples() {
    let formatter = OmlFormatter::new();
    let samples = std::fs::read_dir("project_root/models/oml").expect("list oml samples");
    for entry in samples.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            let content = std::fs::read_to_string(entry.path()).expect("read oml sample");
            let formatted = formatter.format_content_or_original(&content);
            assert!(!formatted.is_empty());
        }
    }
}
