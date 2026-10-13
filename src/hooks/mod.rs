//! Hook installation and lifecycle management for AI coding agents.

pub mod constants;
// Shares `hook_cmd`'s constraint: it runs inside the hook, where stray output
// corrupts the JSON protocol. `from_agent` is the one exception and carries its
// own allow -- it is reached only from the `rtk hook check` CLI.
#[deny(clippy::print_stdout, clippy::print_stderr)]
pub mod decision;
pub mod hook_audit_cmd;
pub mod hook_check;
#[deny(clippy::print_stdout, clippy::print_stderr)]
pub mod hook_cmd;
pub mod init;
pub mod integrity;
pub mod permissions;
pub mod permissions_opencode;
#[deny(clippy::print_stdout, clippy::print_stderr)]
#[cfg(feature = "engine")]
pub mod posthook;
pub mod rewrite_cmd;
#[deny(clippy::print_stdout, clippy::print_stderr)]
pub mod session_start;
pub mod trust;
pub mod verify_cmd;

fn is_rtk_binary(binary: &str) -> bool {
    let binary_name = binary.rsplit(['/', '\\']).next().unwrap_or(binary);
    matches!(binary_name, "rtk" | "rtk.exe")
}

fn is_rtk_hook_command(command: &str, agent: &str) -> bool {
    let parts = crate::discover::lexer::shell_split(command);
    let [parsed_binary, hook, target] = parts.as_slice() else {
        return false;
    };

    is_rtk_binary(parsed_binary) && hook == "hook" && target == agent
}

pub fn is_claude_hook_command(command: &str) -> bool {
    is_rtk_hook_command(command, "claude")
}

/// The session id of the payload this hook process is handling.
///
/// A hook binary serves exactly one tool call, so one slot is the whole
/// lifetime. Set by the processor once its payload parses; read by the
/// pipeline decision log, which records which session a rule fired in.
static HOOK_SESSION: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Which agent's hook binary is running, for the lookups whose vocabulary is
/// per-agent. One slot for the process, like [`set_hook_session`].
static HOOK_AGENT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

pub(crate) fn set_hook_agent(agent: &str) {
    let _ = HOOK_AGENT.set(Some(agent.to_string()));
}

#[cfg(feature = "engine")]
pub(crate) fn hook_agent() -> Option<String> {
    HOOK_AGENT.get().cloned().flatten()
}

pub(crate) fn set_hook_session(session: Option<String>) {
    let _ = HOOK_SESSION.set(session);
}

#[cfg(feature = "engine")]
pub(crate) fn hook_session() -> Option<String> {
    HOOK_SESSION.get().cloned().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_hook_command_matches_bare_and_absolute_rtk() {
        assert!(is_claude_hook_command("rtk hook claude"));
        assert!(is_claude_hook_command("/opt/homebrew/bin/rtk hook claude"));
        assert!(is_claude_hook_command(
            "\"/opt/homebrew/bin/rtk\" hook claude"
        ));
        assert!(is_claude_hook_command(
            "/Users/jane/My\\ Apps/rtk hook claude"
        ));
    }

    #[test]
    fn claude_hook_command_rejects_other_commands() {
        assert!(!is_claude_hook_command("not-rtk hook claude"));
        assert!(!is_claude_hook_command("/opt/homebrew/bin/rtk hook cursor"));
        assert!(!is_claude_hook_command("echo rtk hook claude"));
    }
}
