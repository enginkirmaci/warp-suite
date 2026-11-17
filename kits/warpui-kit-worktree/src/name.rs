//! Branch and folder names derived from a chat title.
//!
//! A chat's worktree is named after the chat, so the name has to survive a user
//! typing anything: a title becomes `agent/<slug>`, the slug keeps letters and
//! digits, folds everything else into single dashes and stops growing at
//! [`MAX_SLUG`] bytes. The folder name is the branch with its separators
//! flattened, which is why the crate never infers a branch back from a folder —
//! it always reads `git worktree list`.

/// The branch prefix every chat worktree uses.
pub const BRANCH_PREFIX: &str = "agent";

/// Longest slug the crate produces, in bytes of the folded text.
const MAX_SLUG: usize = 48;

/// The branch name for a chat title: `agent/<slug>`.
pub fn branch_name(title: &str) -> String {
    format!("{BRANCH_PREFIX}/{}", slug(title))
}

/// The folder-safe form of a name: lowercase letters and digits, runs of
/// anything else folded into one dash, `chat` when nothing is left.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_dash && !out.is_empty() && out.len() + 1 < MAX_SLUG {
                out.push('-');
            }
            pending_dash = false;
            if out.len() >= MAX_SLUG {
                break;
            }
            out.push(character);
        } else {
            pending_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "chat".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_becomes_a_prefixed_branch() {
        assert_eq!(
            branch_name("Fix the login retry"),
            "agent/fix-the-login-retry"
        );
    }

    #[test]
    fn punctuation_folds_into_single_dashes() {
        assert_eq!(slug("  Fix: the / login!  "), "fix-the-login");
        assert_eq!(slug("a---b"), "a-b");
    }

    #[test]
    fn an_empty_title_still_names_something() {
        assert_eq!(branch_name(""), "agent/chat");
        assert_eq!(slug("!!!"), "chat");
    }

    #[test]
    fn a_long_title_is_cut_without_a_trailing_dash() {
        let long = slug(&"word ".repeat(60));
        assert!(long.len() <= MAX_SLUG, "{long} is {} bytes", long.len());
        assert!(!long.ends_with('-'));
    }

    #[test]
    fn a_branch_flattens_into_one_folder_name() {
        assert_eq!(slug("agent/fix-login"), "agent-fix-login");
        assert_eq!(
            slug(&branch_name("Retry the upload")),
            "agent-retry-the-upload"
        );
    }
}
