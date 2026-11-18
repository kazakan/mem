//! mem — shared project memory for AI agents: decisions, conventions, gotchas, facts and handoffs in one local
//! SQLite file, searched with FTS5 (and optionally EmbeddingGemma). One process per command; no server.
//!
//! Module map (see docs/ARCHITECTURE.md):
//! - `cli`      command-line definitions
//! - `commands` dispatch: brief, list, get, edit, retire, syn, import, index, and the add/search entry points
//! - `search`   ranking, grouping, topic pins, module cards, output format
//! - `write`    duplicate / same-subject checks, add, add-batch JSON lines
//! - `mem`      the open store (`Mem`), embedding helpers, insert
//! - `db`       schema, migrations, connection, row helpers
//! - `text`     tokens, Korean josa, stopwords, title shapes, subjects
//! - `pystr`    Python string semantics the output depends on
//! - `util`     environment, dates, DB path, call log
//! - `emb`      (feature `embed`) llama.cpp embeddings

mod cli;
mod commands;
mod db;
#[cfg(feature = "embed")]
mod emb;
mod mem;
mod pystr;
mod search;
#[cfg(test)]
mod testutil;
mod text;
mod util;
mod write;

/// Names every module uses: external types plus the crate's own items.
mod prelude {
    #[allow(unused_imports)]
    pub(crate) use crate::{cli::*, commands::*, db::*, mem::*, pystr::*, search::*, text::*, util::*, write::*};
    pub(crate) use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
    pub(crate) use indexmap::{IndexMap, IndexSet};
    pub(crate) use regex::Regex;
    pub(crate) use rusqlite::types::{Value, ValueRef};
    pub(crate) use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
    pub(crate) use std::collections::{HashMap, HashSet};
    pub(crate) use std::io::{Read, Write};
    pub(crate) use std::sync::LazyLock;
}

use prelude::*;

fn main() {
    let argv: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    if let Some(msg) = argparse_violation(&argv) {
        eprintln!("mem: error: {msg}");
        std::process::exit(2);
    }
    let cmd = Cli::command().mut_subcommands(|c| c.infer_long_args(true).args_override_self(true));
    let matches = cmd.get_matches_from(std::iter::once("mem".to_string()).chain(argv.iter().cloned()));
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    if matches!(cli.cmd, Cmd::Where) {
        println!("{}", where_text());
        return;
    }
    let path = db_path();
    let mut partial = String::new();
    let out = match run(&path, cli.cmd, &mut partial) {
        Ok(o) => o,
        Err(e) => {
            if !partial.is_empty() {
                let _ = writeln!(std::io::stdout().lock(), "{partial}");
            }
            eprintln!("mem: error: {e}");
            std::process::exit(1);
        }
    };
    {
        // a closed pipe (`mem list | head`) must not crash: write errors are ignored
        let mut so = std::io::stdout().lock();
        let _ = writeln!(so, "{out}");
        let _ = so.flush();
    }
    log_call(&path, &argv, clen(&out));
}
