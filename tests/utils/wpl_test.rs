use wp_model_core::model::{DataField, DataRecord, DataType, Value};
use wp_station::utils::{
    warp_check_record,
    wpl::{WplFormatter, record_to_fields},
};

fn sample_files_with_extension(root: &str, extension: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some(extension)
            {
                files.push(path);
            }
        }
    }

    files
}

fn sample_record() -> DataRecord {
    DataRecord::from(vec![
        DataField::new(DataType::Chars, "alpha", Value::from("one")),
        DataField::new(DataType::Chars, "beta", Value::from("two")),
    ])
}

#[test]
fn test_record_to_fields_assigns_sequential_numbers() {
    let record = sample_record();
    let fields = record_to_fields(&record);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].no, 1);
    assert_eq!(fields[0].name, "alpha");
    assert_eq!(fields[1].value, "two");
}

#[test]
fn test_wpl_formatter_formats_code() {
    let formatter = WplFormatter::new();
    let messy = "package test { rule sample { ( digit:status, chars:message ) } }";
    let formatted = formatter.format_content(messy).expect("format wpl content");
    assert!(formatted.contains("package test"));
    assert!(
        formatted
            .lines()
            .any(|line| line.trim_start().starts_with("rule"))
    );
}

#[test]
fn test_wpl_formatter_handles_annotations_and_comments() {
    let formatter = WplFormatter::new();
    let source = r#"
        package demo {
            #[tag(example)]
            rule annotated {
                (
                    chars:name, // inline comment
                    raw("value")
                )
            }
        }
    "#;
    let formatted = formatter
        .format_content(source)
        .expect("format annotated wpl");
    assert!(formatted.contains("#[tag(example)]"));
    assert!(formatted.contains("chars:name"));
    assert!(formatted.contains("raw(\"value\")"));
}

#[test]
fn test_wpl_formatter_handles_raw_strings_and_quotes() {
    let formatter = WplFormatter::new();
    let source =
        r##"package raw_demo { rule sample { ( chars:"value,with,comma", r#"raw(content)"# ) } }"##;
    let formatted = formatter.format_content(source).expect("format raw string");
    assert!(formatted.contains("raw_demo"));
    assert!(formatted.contains("raw(content)"));
}

#[test]
fn test_wpl_formatter_format_content_or_original_on_error() {
    let formatter = WplFormatter::new();
    let source = "package bad { rule broken { ( digit:id ";
    let output = formatter.format_content_or_original(source);
    assert_eq!(output, source);
}

#[test]
fn test_wpl_formatter_preserves_raw_functions() {
    let formatter = WplFormatter::new();
    let source = r#"
        package demo {
            rule raw_funcs {
                symbol("a|b|c") | f_chars_in("abc")
            }
        }
    "#;
    let formatted = formatter
        .format_content(source)
        .expect("format raw functions");
    assert!(formatted.contains("symbol(\"a|b|c\")"));
    assert!(formatted.contains("f_chars_in(\"abc\")"));
}

#[test]
fn test_wpl_formatter_reports_unbalanced_brackets() {
    let formatter = WplFormatter::new();
    let result = formatter.format_content("package demo { rule broken { ( digit:id } }");
    assert!(result.is_err());
}

#[test]
fn test_wpl_formatter_formats_project_samples() {
    let formatter = WplFormatter::new();
    let samples = sample_files_with_extension("project_root/models/wpl", "wpl");
    assert!(!samples.is_empty(), "expected at least one WPL sample");

    for path in samples {
        let content = std::fs::read_to_string(&path).expect("read wpl sample");
        let formatted = formatter.format_content_or_original(&content);
        assert!(!formatted.is_empty(), "empty formatted output for {path:?}");
    }
}

#[test]
fn test_warp_check_record_errors_without_rules() {
    let wpl = "package empty {}";
    assert!(
        warp_check_record(wpl, "data").is_err(),
        "expected warp_check_record to error when no rules"
    );
}

#[test]
fn test_warp_check_record_invalid_package() {
    let wpl = "package";
    let err = warp_check_record(wpl, "data").expect_err("invalid wpl");
    assert!(format!("{:?}", err).contains("WPL 包解析错误"));
}
