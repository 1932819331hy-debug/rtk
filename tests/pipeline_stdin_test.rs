#![cfg(unix)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

mod common;

fn run_with_stdin(command: &mut Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn command");
    let mut stdin = child.stdin.take().expect("piped stdin");
    // A command that rejects its arguments exits before it reads stdin, so this
    // write races that exit: EPIPE is a normal outcome, not a failure. What the
    // caller asserts on is the exit status, which survives either way.
    match stdin.write_all(input) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => panic!("write stdin: {e:?}"),
    }
    drop(stdin);
    child.wait_with_output().expect("wait for command")
}

#[test]
fn early_exit_preserves_output_and_status_when_stdin_is_closed() {
    // More than a pipe buffer ensures the writer observes the closed reader,
    // regardless of whether the child exits before or during write_all.
    let input = vec![b'x'; 8 * 1024 * 1024];
    let output = run_with_stdin(
        Command::new("sh").args(["-c", "printf 'early exit\\n'; exit 7"]),
        &input,
    );

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"early exit\n");
}

#[test]
fn wc_reads_piped_stdin() {
    let output = run_with_stdin(common::rtk_command().args(["wc", "-l"]), b"alpha\nbeta\n");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "2");
}

#[test]
fn wc_reads_explicit_stdin_alongside_file_operand() {
    let dir = common::tempdir();
    std::fs::write(dir.path().join("input.txt"), b"file one\nfile two\n")
        .expect("write scratch input");
    std::fs::write(dir.path().join("other.txt"), b"one\ntwo\nthree\nfour\n")
        .expect("write second scratch input");
    let input = b"pipe one\npipe two\npipe three\n";

    let cases: &[(&[&str], &[u64])] = &[
        (&["input.txt", "-"], &[2, 3, 5]),
        (&["-", "input.txt"], &[3, 2, 5]),
        (&[], &[3]),
        (&["-"], &[3]),
        (&["input.txt"], &[2]),
        (&["input.txt", "other.txt"], &[2, 4, 6]),
        (&["input.txt", "-", "other.txt"], &[2, 3, 4, 9]),
        (&["input.txt", "-", "-"], &[2, 3, 0, 5]),
    ];
    for &(operands, expected) in cases {
        let native = run_with_stdin(
            common::native_command("wc")
                .arg("-l")
                .args(operands)
                .current_dir(dir.path())
                .env("LC_ALL", "C"),
            input,
        );
        let rtk = run_with_stdin(
            common::rtk_command()
                .args(["wc", "-l"])
                .args(operands)
                .current_dir(dir.path())
                .env("LC_ALL", "C"),
            input,
        );
        let counts = |output: &[u8]| {
            String::from_utf8_lossy(output)
                .lines()
                .map(|line| {
                    line.split_whitespace()
                        .find_map(|word| word.parse::<u64>().ok())
                        .expect("wc output row has a count")
                })
                .collect::<Vec<_>>()
        };

        assert!(native.status.success());
        assert_eq!(counts(&native.stdout), expected);
        assert_eq!(rtk.status.code(), native.status.code());
        assert_eq!(
            counts(&rtk.stdout),
            counts(&native.stdout),
            "operands {operands:?}; rtk stdout: {}",
            String::from_utf8_lossy(&rtk.stdout)
        );
    }
}

#[test]
fn wc_preserves_native_failure_exit_code() {
    let invalid_option = "--definitely-invalid-rtk-test-option";
    let rtk = run_with_stdin(
        common::rtk_command().args(["wc", invalid_option]),
        b"input\n",
    );
    let native = run_with_stdin(common::native_command("wc").arg(invalid_option), b"input\n");

    assert!(!rtk.status.success());
    assert_eq!(rtk.status.code(), native.status.code());
}
