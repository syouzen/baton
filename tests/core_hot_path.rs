use baton_core::{ByteRing, Coalescer, Scrollback, Watermark};
use std::time::Duration;

#[test]
fn byte_ring_applies_backpressure_without_dropping_bytes() {
    let mut ring = ByteRing::new(8);

    assert_eq!(ring.push(b"hello"), Ok(5));
    assert_eq!(ring.push(b" world"), Err(Watermark::Full { accepted: 3 }));
    assert_eq!(ring.len(), 8);
    assert_eq!(ring.drain_all(), b"hello wo");
}

#[test]
fn coalescer_flushes_on_frame_interval_or_capacity() {
    let mut coalescer = Coalescer::new(16, Duration::from_millis(8));

    coalescer
        .ingest_at(b"abc", Duration::from_millis(0))
        .unwrap();
    assert!(!coalescer.should_flush(Duration::from_millis(7)));
    assert!(coalescer.should_flush(Duration::from_millis(8)));
    assert_eq!(coalescer.flush(), b"abc");

    coalescer
        .ingest_at(b"0123456789abcdef", Duration::from_millis(9))
        .unwrap();
    assert!(coalescer.should_flush(Duration::from_millis(9)));
}

#[test]
fn scrollback_keeps_recent_lines_and_spills_older_lines() {
    let mut scrollback = Scrollback::new(3);

    for n in 0..5 {
        scrollback.push_line(format!("line-{n}")).unwrap();
    }

    assert_eq!(scrollback.visible_lines(), &["line-2", "line-3", "line-4"]);
    assert_eq!(scrollback.spilled_lines(), vec!["line-0", "line-1"]);
    assert_eq!(scrollback.total_lines(), 5);
}
