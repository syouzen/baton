use baton_core::LocalPty;
use std::time::Duration;

#[test]
fn local_pty_captures_child_output_through_coalesced_reader() {
    let mut pty = LocalPty::spawn("/bin/sh", &["-lc", "printf baton-ready"]).unwrap();

    let output = pty.read_available(Duration::from_secs(2)).unwrap();

    assert_eq!(String::from_utf8_lossy(&output), "baton-ready");
    assert!(pty.wait().unwrap().success());
}
