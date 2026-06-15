#![cfg(unix)]

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// Remove ANSI / VT escape sequences from a raw terminal byte stream.
///
/// This lets assertions work on logical text content even when the renderer
/// (ratatui) uses differential redraws that interleave cursor-positioning
/// codes with short text fragments.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek() {
                // CSI sequence: ESC [ ... <final byte in 0x40-0x7E>
                Some('[') => {
                    chars.next(); // consume '['
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break; // final byte consumed
                        }
                    }
                }
                // Two-byte sequences: ESC ( ESC ) ESC # etc.
                Some(_) => {
                    chars.next(); // consume the second byte
                }
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Boots the real binary in a real PTY: the TTY gate must choose the TUI,
/// the header must render, and `q` must exit 0. This is the only test that
/// exercises the TTY branch — assert_cmd pipes stdout everywhere else.
#[test]
fn tui_boots_renders_header_and_quits_clean() {
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cache = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_skillhealth"));
    cmd.args([
        "--config-dir",
        fixtures.join("config").to_str().unwrap(),
        "--projects-dir",
        fixtures.join("projects").to_str().unwrap(),
        "--cache-dir",
        cache.path().to_str().unwrap(),
        "--now",
        "2026-06-10T12:00:00Z",
    ]);
    cmd.cwd(cwd.path());
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    // Reader thread: stream PTY output, signal once the header shows up.
    let mut reader = pair.master.try_clone_reader().unwrap();
    let (tx, rx) = mpsc::channel::<()>();
    let collector = std::thread::spawn(move || {
        let mut seen = String::new();
        let mut buf = [0u8; 4096];
        let mut signaled = false;
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            seen.push_str(&String::from_utf8_lossy(&buf[..n]));
            if !signaled && seen.contains("skillhealth") && seen.contains("skills") {
                signaled = true;
                let _ = tx.send(());
            }
        }
        seen
    });

    rx.recv_timeout(Duration::from_secs(20))
        .expect("TUI header did not render within 20s");

    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();

    let status = child.wait().unwrap();
    assert!(status.success(), "expected exit 0 on q, got {status:?}");
    drop(pair.master);
    let _ = collector.join();
}

/// p cycles the scope (all → user on the project-less fixture cwd) and L
/// flips the lens — both must show up in the redrawn header.
#[test]
fn pty_p_and_l_update_the_header() {
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 30,
            cols: 160,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cache = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_skillhealth"));
    cmd.args([
        "--config-dir",
        fixtures.join("config").to_str().unwrap(),
        "--projects-dir",
        fixtures.join("projects").to_str().unwrap(),
        "--cache-dir",
        cache.path().to_str().unwrap(),
        "--now",
        "2026-06-10T12:00:00Z",
    ]);
    cmd.cwd(cwd.path());
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let (tx, rx) = mpsc::channel::<()>();
    let collector = std::thread::spawn(move || {
        let mut seen: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        let mut signaled = false;
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if !signaled
                && seen
                    .windows(b"scope: all".len())
                    .any(|w| w == b"scope: all")
            {
                signaled = true;
                let _ = tx.send(());
            }
        }
        seen
    });

    rx.recv_timeout(Duration::from_secs(20))
        .expect("initial header with scope indicator did not render within 20s");

    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(b"p").unwrap(); // all → user (scope cycle)
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    writer.write_all(b"L").unwrap(); // lens global → project
    writer.flush().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();

    let status = child.wait().unwrap();
    assert!(status.success(), "expected exit 0 on q, got {status:?}");
    drop(pair.master);
    let seen_bytes = collector.join().unwrap();
    // Convert once after all bytes are accumulated — avoids replacement chars
    // from multi-byte UTF-8 sequences (e.g. U+00B7 = 0xC2 0xB7) being split
    // across PTY read chunks.
    let seen = String::from_utf8_lossy(&seen_bytes).into_owned();

    // Strip ANSI/VT escape sequences so we can assert on logical text content
    // even when ratatui uses differential (partial) redraws.  The accumulated
    // PTY stream is a sequence of raw terminal bytes; removing all ESC-introduced
    // control sequences collapses every frame's characters into a single string
    // that contains every label ever rendered.
    let plain = strip_ansi(&seen);

    assert!(
        plain.contains("lens: global"),
        "initial lens missing:\n{plain}"
    );
    // After `p` ratatui does a differential redraw: only the changed span is
    // rewritten.  "scope: " was already on-screen so only "user · lens:" is
    // emitted.  That fragment is unique and proves scope cycled to User.
    assert!(
        plain.contains("user · lens:"),
        "p did not cycle scope:\n{plain}"
    );
    // After `L` ratatui rewrites only the characters that differ between
    // "global" and "project".  Positions that match ('o' at index 2 of both
    // words) are skipped, producing "prject" in the stripped stream — the 'o'
    // shared by both labels is elided.  This 6-char fragment is unique to the
    // global→project transition and proves L fired.  If a future ratatui
    // version does a full redraw instead, "lens: project" appears contiguously.
    assert!(
        plain.contains("prject") || plain.contains("lens: project"),
        "L did not flip lens (expected 'prject' diff fragment or 'lens: project'):\n{plain}"
    );
}
