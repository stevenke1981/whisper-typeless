use whisper_typeless::output::formatter::{FormatterConfig, OutputFormatter};

#[test]
fn formatter_appends_newline() {
    let config = FormatterConfig {
        append_newline: true,
        append_space: false,
        add_timestamp: false,
    };
    let f = OutputFormatter::new(config);
    let result = f.format("Hello");
    assert!(result.ends_with('\n'));
}

#[test]
fn formatter_no_append() {
    let config = FormatterConfig {
        append_newline: false,
        append_space: false,
        add_timestamp: false,
    };
    let f = OutputFormatter::new(config);
    let result = f.format("Hello");
    assert_eq!(result, "Hello");
}

#[test]
fn formatter_appends_space() {
    let config = FormatterConfig {
        append_newline: false,
        append_space: true,
        add_timestamp: false,
    };
    let f = OutputFormatter::new(config);
    let result = f.format("Hello");
    assert!(result.ends_with(' '));
}
