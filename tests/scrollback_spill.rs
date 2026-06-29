use baton_core::{InMemorySpill, Scrollback, ScrollbackSpill};

#[test]
fn scrollback_uses_spill_trait_when_visible_cap_is_exceeded() {
    let mut scrollback = Scrollback::with_spill(2, InMemorySpill::default());

    scrollback.push_line("one").unwrap();
    scrollback.push_line("two").unwrap();
    scrollback.push_line("three").unwrap();
    scrollback.push_line("four").unwrap();

    assert_eq!(
        scrollback.visible_lines(),
        &["three".to_string(), "four".to_string()]
    );
    assert_eq!(
        scrollback.spilled_lines(),
        vec!["one".to_string(), "two".to_string()]
    );
    assert_eq!(scrollback.spill_len(), 2);
    assert_eq!(scrollback.total_lines(), 4);
}

#[test]
fn scrollback_reports_spill_write_errors_without_losing_visible_line() {
    #[derive(Debug, Default)]
    struct FailingSpill;

    impl ScrollbackSpill for FailingSpill {
        fn append_line(&mut self, _line: String) -> anyhow::Result<()> {
            anyhow::bail!("spill failed")
        }

        fn len(&self) -> usize {
            0
        }

        fn lines(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let mut scrollback = Scrollback::with_spill(1, FailingSpill);
    scrollback.push_line("kept").unwrap();

    let err = scrollback.push_line("new").unwrap_err();

    assert!(err.to_string().contains("spill failed"));
    assert_eq!(scrollback.visible_lines(), &["kept".to_string()]);
    assert_eq!(scrollback.total_lines(), 1);
}
