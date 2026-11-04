//! tree command - proxy to native tree with token-optimized output
//!
//! This module proxies to the native `tree` command and filters the output
//! to reduce token usage while preserving structure visibility.
//!
//! Token optimization: automatically excludes noise directories via -I pattern
//! unless -a flag is present (respecting user intent).

use super::constants::NOISE_DIRS;
use crate::core::arg_tokenizer::{self, Dialect, TokenKind, ValueSpec};
use crate::core::runner::{self, RunOptions};
use crate::core::shell::display_args;
use crate::core::utils::{ChildArgExt, resolved_command, tool_exists};
use anyhow::Result;

pub fn run(args: &[String], verbose: u8) -> Result<i32> {
    if !tool_exists("tree") {
        anyhow::bail!(
            "tree command not found. Install it first:\n\
             - macOS: brew install tree\n\
             - Ubuntu/Debian: sudo apt install tree\n\
             - Fedora/RHEL: sudo dnf install tree\n\
             - Arch: sudo pacman -S tree"
        );
    }

    let mut cmd = resolved_command("tree");

    let show_all = args.iter().any(|a| a == "-a" || a == "--all");
    let has_ignore = args.iter().any(|a| a == "-I" || a.starts_with("--ignore="));

    if !show_all && !has_ignore {
        let ignore_pattern = NOISE_DIRS.join("|");
        cmd.arg("-I").arg(&ignore_pattern);
    }

    cmd.child_args(args);

    runner::run_filtered(
        cmd,
        "tree",
        &display_args(args),
        |raw| {
            let filtered = filter_tree_output(raw, args);
            if verbose > 0 {
                eprintln!(
                    "Lines: {} → {} ({}% reduction)",
                    raw.lines().count(),
                    filtered.lines().count(),
                    if raw.lines().count() > 0 {
                        100 - (filtered.lines().count() * 100 / raw.lines().count())
                    } else {
                        0
                    }
                );
            }
            filtered
        },
        RunOptions::stdout_only()
            .early_exit_on_failure()
            .no_trailing_newline(),
    )
}

fn tree_has_report(args: &[String]) -> bool {
    let tokens = arg_tokenizer::tokenize_grammar(
        args,
        &|kind, name| {
            let takes_value = match kind {
                TokenKind::Short => matches!(name, "L" | "H" | "T" | "o" | "P" | "I"),
                TokenKind::Long => matches!(
                    name,
                    "gitfile"
                        | "infofile"
                        | "charset"
                        | "filelimit"
                        | "timefmt"
                        | "sort"
                        | "hintro"
                        | "houtro"
                        | "scheme"
                        | "authority"
                ),
                _ => false,
            };
            takes_value.then(|| ValueSpec::value().claiming_dash_dash())
        },
        Dialect::Posix,
    );
    !arg_tokenizer::has_flag(&tokens, Dialect::Posix, "noreport")
}

fn filter_tree_output(raw: &str, args: &[String]) -> String {
    // With -N, filenames can contain literal newlines and imitate even a complete
    // report. When reporting is disabled, every byte belongs to the listing.
    if !tree_has_report(args) {
        return raw.to_string();
    }

    let Some((body, summary)) = raw.trim_end_matches(['\r', '\n']).rsplit_once('\n') else {
        return raw.to_string();
    };
    let body = body.strip_suffix('\r').unwrap_or(body);
    // Native tree separates its final count from the listing with an empty line.
    // Keep the listing's own line endings, including newlines inside filenames.
    if !body.ends_with('\n') || body.trim().is_empty() {
        return raw.to_string();
    }
    let Some((directories, files)) = summary
        .split_once(" directory, ")
        .or_else(|| summary.split_once(" directories, "))
    else {
        return raw.to_string();
    };
    let Some(files) = files
        .strip_suffix(" file")
        .or_else(|| files.strip_suffix(" files"))
    else {
        return raw.to_string();
    };
    if arg_tokenizer::is_digit_run(directories) && arg_tokenizer::is_digit_run(files) {
        body.to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_removes_summary() {
        let input = ".\n├── src\n│   └── main.rs\n└── Cargo.toml\n\n2 directories, 3 files\n";
        let output = filter_tree_output(input, &[]);
        assert!(!output.contains("directories"));
        assert!(!output.contains("files"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("Cargo.toml"));
    }

    #[test]
    fn test_filter_preserves_structure() {
        let input = ".\n├── src\n│   ├── main.rs\n│   └── lib.rs\n└── tests\n    └── test.rs\n";
        let output = filter_tree_output(input, &[]);
        assert!(output.contains("├──"));
        assert!(output.contains("│"));
        assert!(output.contains("└──"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("test.rs"));
    }

    #[test]
    fn test_filter_preserves_names_containing_summary_words() {
        let input = "files-directory\n|-- file-directory.txt\n|-- plain.txt\n`-- subdirectory-files\n\n2 directories, 2 files\n";
        assert_eq!(
            filter_tree_output(input, &[]),
            "files-directory\n|-- file-directory.txt\n|-- plain.txt\n`-- subdirectory-files\n"
        );
    }

    #[test]
    fn test_filter_preserves_summary_shaped_filename_lines() {
        // Native `tree -N` prints the embedded newlines literally.
        let input = "plain\n`-- prefix\n\n1 directory, 1 file\n\n1 directory, 1 file\n";
        assert_eq!(
            filter_tree_output(input, &[]),
            "plain\n`-- prefix\n\n1 directory, 1 file\n"
        );
        // Without the report separator, the last line can be a `tree -i` entry.
        let no_separator = "plain\n1 directory, 1 file\n";
        assert_eq!(filter_tree_output(no_separator, &[]), no_separator);
        let root_only = "1 directory, 1 file\n";
        assert_eq!(filter_tree_output(root_only, &[]), root_only);
    }

    #[test]
    fn test_filter_preserves_filename_trailing_newlines() {
        let input = "plain\n`-- suffix\n\n\n\n1 directory, 1 file\n";
        assert_eq!(filter_tree_output(input, &[]), "plain\n`-- suffix\n\n\n");
    }

    #[test]
    fn test_filter_preserves_literal_filename_with_noreport() {
        let input = "plain\n`-- prefix\n\n1 directory, 1 file\n";
        let args = vec!["-N".to_string(), "--noreport".to_string()];
        assert_eq!(filter_tree_output(input, &args), input);
    }

    #[test]
    fn test_noreport_values_and_paths_do_not_disable_report_filtering() {
        let input = "plain\n`-- file.txt\n\n1 directory, 1 file\n";
        for args in [
            vec!["-I", "--noreport"],
            vec!["-P--noreport"],
            vec!["--timefmt", "--noreport"],
            vec!["--timefmt=--noreport"],
            vec!["--", "--noreport"],
            vec!["-I", "--", "--noreport"],
        ] {
            let expected = if args == ["-I", "--", "--noreport"] {
                input
            } else {
                "plain\n`-- file.txt\n"
            };
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            assert_eq!(filter_tree_output(input, &args), expected, "{args:?}");
        }
    }

    #[test]
    fn test_filter_preserves_unrecognized_output() {
        for input in [
            ".\n\n1 directory, 1 file.txt\n",
            ".\n\none directory, 2 files\n",
            ".\n\n1 directory, 2 files trailing text\n",
            ".\n\n1 Verzeichnis, 2 Dateien\n",
            "[{\"type\":\"file\",\"name\":\"directory.txt\"}]\n",
        ] {
            assert_eq!(filter_tree_output(input, &[]), input);
        }
    }

    #[test]
    fn test_filter_preserves_crlf_listing() {
        let input = "plain\r\n`-- file-directory.txt\r\n\r\n1 directory, 1 file\r\n";
        assert_eq!(
            filter_tree_output(input, &[]),
            "plain\r\n`-- file-directory.txt\r\n"
        );
    }

    #[test]
    fn test_filter_savings_on_native_tree_output() {
        let input = "plain\n`-- ordinary.txt\n\n1 directory, 1 file\n";
        let output = filter_tree_output(input, &[]);
        assert_eq!(output, "plain\n`-- ordinary.txt\n");
        assert!((input.len() - output.len()) * 100 >= input.len() * 20);
    }

    #[test]
    fn test_filter_handles_empty() {
        let input = "";
        let output = filter_tree_output(input, &[]);
        assert_eq!(output, "");
    }

    #[test]
    fn test_filter_preserves_trailing_empty_lines_without_report() {
        let input = ".\n├── file.txt\n\n\n";
        let output = filter_tree_output(input, &[]);
        assert_eq!(output, input);
    }

    #[test]
    fn test_filter_summary_variations() {
        // Test different summary formats
        let inputs = vec![
            (".\n└── file.txt\n\n0 directories, 1 file\n", "1 file"),
            (".\n└── file.txt\n\n1 directory, 0 files\n", "1 directory"),
            (".\n└── file.txt\n\n10 directories, 25 files\n", "25 files"),
        ];

        for (input, summary_fragment) in inputs {
            let output = filter_tree_output(input, &[]);
            assert!(
                !output.contains(summary_fragment),
                "Should remove summary '{}' from output",
                summary_fragment
            );
            assert!(
                output.contains("file.txt"),
                "Should preserve file.txt in output"
            );
        }
    }

    #[test]
    fn test_noise_dirs_constant() {
        // Verify NOISE_DIRS contains expected patterns
        assert!(NOISE_DIRS.contains(&"node_modules"));
        assert!(NOISE_DIRS.contains(&".git"));
        assert!(NOISE_DIRS.contains(&"target"));
        assert!(NOISE_DIRS.contains(&"__pycache__"));
        assert!(NOISE_DIRS.contains(&".next"));
        assert!(NOISE_DIRS.contains(&"dist"));
        assert!(NOISE_DIRS.contains(&"build"));
    }
}
