//! Following the format diagnostic's recovery hint must enter write mode.

mod common;

#[cfg(unix)]
#[test]
fn format_recovery_hint_runs_in_write_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = common::tempdir();
    let stub = dir.path().join("dotnet");
    std::fs::write(
        &stub,
        r#"#!/bin/sh
printf '%s\n' "$@" > argv.txt
verify=false
report=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --verify-no-changes) verify=true ;;
        --report) shift; report=$1 ;;
    esac
    shift
done
cp format_changes.json "$report"
# Supply enough native diagnostic output for RTK to retain its compact summary.
i=0
while [ "$i" -lt 30 ]; do
    printf 'src/Program.cs(42,17): error WHITESPACE: Fix whitespace formatting.\n'
    i=$((i + 1))
done
if [ "$verify" = true ]; then
    exit 2
fi
printf 'formatted\n' > applied.txt
"#,
    )
    .expect("write dotnet stub");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
        .expect("make dotnet stub executable");
    std::fs::write(
        dir.path().join("format_changes.json"),
        include_str!("fixtures/dotnet/format_changes.json"),
    )
    .expect("write format report fixture");
    let path = std::env::join_paths(std::iter::once(dir.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("prepend dotnet stub to PATH");
    let run = |args: &[&str]| {
        common::rtk_command()
            .env("PATH", &path)
            .current_dir(dir.path())
            .args(args)
            .output()
            .expect("run rtk")
    };

    let checked = run(&["dotnet", "format"]);
    assert_eq!(checked.status.code(), Some(2));
    assert!(!dir.path().join("applied.txt").exists());
    let stdout = String::from_utf8_lossy(&checked.stdout);
    let command = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Run `")?.strip_suffix("` to apply fixes"))
        .expect("format output should include an actionable recovery command");
    let args: Vec<_> = command.split_whitespace().collect();
    assert_eq!(
        args.first(),
        Some(&"rtk"),
        "hint must select RTK write mode"
    );
    let written = run(&args[1..]);
    assert!(written.status.success(), "{written:?}");
    assert!(dir.path().join("applied.txt").exists());
    let argv = std::fs::read_to_string(dir.path().join("argv.txt")).expect("read dotnet argv");
    assert!(argv.lines().any(|arg| arg == "format"));
    assert!(!argv.lines().any(|arg| arg == "--verify-no-changes"));
    assert!(!argv.lines().any(|arg| arg == "--write"));
}
