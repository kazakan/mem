//! Command-line definitions (clap) and argparse-compatible checks.

use crate::prelude::*;

#[derive(Parser)]
#[command(name = "mem", about = "shared project memory (decisions, conventions, gotchas, facts, handoffs)", version)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// session start: counts, latest handoffs, recently changed notes
    Brief {
        #[arg(long, allow_hyphen_values = true)]
        project: String,
    },
    /// search current notes
    Search {
        /// the request or question, as-is
        #[arg(long, allow_hyphen_values = true)]
        q: String,
        /// extra keywords, comma separated (synonyms, English/Korean spellings)
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        kw: String,
        #[arg(long, allow_hyphen_values = true)]
        project: Option<String>,
        /// only these kinds, comma separated
        #[arg(long, allow_hyphen_values = true)]
        kind: Option<String>,
        #[arg(long, allow_negative_numbers = true)]
        limit: Option<i64>,
    },
    /// list notes (for overviews like 'all current conventions')
    List {
        #[arg(long, allow_hyphen_values = true)]
        project: Option<String>,
        /// comma separated
        #[arg(long, allow_hyphen_values = true)]
        kind: Option<String>,
        /// current|superseded|retired|all
        #[arg(long, default_value = "current", allow_hyphen_values = true)]
        status: String,
        #[arg(long, allow_hyphen_values = true)]
        tag: Option<String>,
        /// user|agent
        #[arg(long, allow_hyphen_values = true)]
        source: Option<String>,
        /// also print summaries
        #[arg(long)]
        full: bool,
        #[arg(long, default_value_t = 200, allow_negative_numbers = true)]
        limit: i64,
    },
    /// print full notes
    Get {
        #[arg(required = true, allow_negative_numbers = true)]
        ids: Vec<String>,
    },
    /// save a new note
    Add {
        #[arg(long, allow_hyphen_values = true)]
        project: String,
        #[arg(long, allow_hyphen_values = true)]
        kind: String,
        #[arg(long, allow_hyphen_values = true)]
        title: String,
        #[arg(long, allow_hyphen_values = true)]
        summary: String,
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        body: String,
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        tags: String,
        /// id(s) this note replaces, comma separated
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        supersedes: String,
        /// user = the user said it; agent = your own choice
        #[arg(long, default_value = "user", value_parser = ["user", "agent"], allow_hyphen_values = true)]
        source: String,
        /// save even if it looks like a duplicate
        #[arg(long)]
        force: bool,
        /// a current note on the same subject stays true too (the user added, not replaced)
        #[arg(long = "keep-both")]
        keep_both: bool,
    },
    /// save many notes from a JSON-lines file ('-' = stdin)
    #[command(name = "add-batch")]
    AddBatch {
        #[arg(long, allow_hyphen_values = true)]
        project: String,
        /// dry run: show duplicates and similar current notes for each line, save nothing
        #[arg(long)]
        check: bool,
        file: String,
    },
    /// change a note in place (small fixes; use add --supersedes when a decision changes)
    Edit {
        #[arg(allow_negative_numbers = true)]
        id: String,
        #[arg(long, allow_hyphen_values = true)]
        title: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        summary: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        body: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        tags: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        kind: Option<String>,
        /// append text to the body
        #[arg(long, allow_hyphen_values = true)]
        append: Option<String>,
    },
    /// hide notes that are wrong, junk or duplicates (kept for history)
    Retire {
        #[arg(required = true, allow_negative_numbers = true)]
        ids: Vec<String>,
        #[arg(long, allow_hyphen_values = true)]
        reason: String,
    },
    /// manage search synonyms: mem syn add <word> <word> ...
    Syn {
        #[arg(value_parser = ["add", "show"])]
        action: String,
        #[arg(allow_negative_numbers = true)]
        words: Vec<String>,
    },
    /// load notes from a JSON-lines file with every column, ids kept (moving or merging stores)
    Import { path: String },
    /// (embed build only) embed notes that have no embedding yet
    Index,
    /// print the store path this call would use, the model path and the build; creates nothing
    Where,
}

pub(crate) const VALUE_OPTS: &[&str] = &[
    "--project",
    "--q",
    "--kw",
    "--kind",
    "--limit",
    "--status",
    "--tag",
    "--source",
    "--title",
    "--summary",
    "--body",
    "--tags",
    "--supersedes",
    "--reason",
    "--append",
];

/// The argparse rule (kept from the Python original): after an option that takes a value, a token that looks like
/// an option is an error (starts with '-', longer than 1, not a negative number, no space). Values such as
/// "- bullet text" are fine. Returns the error message, or None.
pub(crate) fn argparse_violation(argv: &[String]) -> Option<String> {
    let neg = |t: &str| Regex::new(r"^-\d+$|^-\d*\.\d+$").unwrap().is_match(t);
    let optlike = |t: &str| t.starts_with('-') && t.len() > 1 && !neg(t) && !t.contains(' ');
    for (i, t) in argv.iter().enumerate() {
        if !t.starts_with("--") || t.contains('=') {
            continue;
        }
        let hits: Vec<&&str> = VALUE_OPTS.iter().filter(|o| *o == t || o.starts_with(t.as_str())).collect();
        if hits.len() == 1 || VALUE_OPTS.contains(&t.as_str()) {
            let o = if VALUE_OPTS.contains(&t.as_str()) { t.as_str() } else { hits[0] };
            match argv.get(i + 1) {
                Some(n) if !optlike(n) => {}
                _ => return Some(format!("argument {o}: expected one argument")),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn option_followed_by_option_is_an_error() {
        assert_eq!(
            argparse_violation(&v(&["add", "--tags", "--force"])),
            Some("argument --tags: expected one argument".into())
        );
        assert!(argparse_violation(&v(&["search", "--q"])).is_some());
        // abbreviations count, like argparse
        assert_eq!(argparse_violation(&v(&["add", "--sum"])), Some("argument --summary: expected one argument".into()));
    }

    #[test]
    fn values_that_start_with_a_dash_are_allowed_when_argparse_allows_them() {
        assert_eq!(argparse_violation(&v(&["add", "--body", "- rule one (since 2026-01-01)"])), None);
        assert_eq!(argparse_violation(&v(&["search", "--q", "x", "--limit", "-1"])), None);
        assert_eq!(argparse_violation(&v(&["search", "--q=--x"])), None);
    }
}
