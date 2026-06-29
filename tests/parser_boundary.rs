use baton_core::{TerminalParser, TerminalSize};

#[test]
fn parser_snapshot_contains_plain_text_lines() {
    let mut parser = TerminalParser::new(TerminalSize::new(5, 20).unwrap());

    parser.advance_bytes(b"hello\r\nworld");
    let snapshot = parser.snapshot();

    assert_eq!(snapshot.size().rows, 5);
    assert_eq!(snapshot.size().cols, 20);
    assert_eq!(snapshot.line_text(0), "hello");
    assert_eq!(snapshot.line_text(1), "world");
}

#[test]
fn parser_applies_carriage_return_and_cursor_movement() {
    let mut parser = TerminalParser::new(TerminalSize::new(3, 10).unwrap());

    parser.advance_bytes(b"hello\rY\x1b[2CY");
    let snapshot = parser.snapshot();

    assert_eq!(snapshot.line_text(0), "YelYo");
}

#[test]
fn terminal_size_rejects_zero_dimensions() {
    assert!(TerminalSize::new(0, 10).is_err());
    assert!(TerminalSize::new(10, 0).is_err());
}
