//! Shared helpers for integration tests: a temp store and a way to run the built `mem` binary against it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_mem")
}

pub struct Out {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Out {
    /// first word after "saved " in an add / add-batch line
    pub fn saved_id(&self) -> String {
        let i = self.out.find("saved ").unwrap_or_else(|| panic!("not saved:\n{}\n{}", self.out, self.err));
        self.out[i + 6..].split_whitespace().next().unwrap().to_string()
    }
}

pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new() -> Store {
        let dir = std::env::temp_dir().join(format!(
            "mem-it-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Store { dir }
    }

    /// a store that starts as a copy of a fixture database
    pub fn from_fixture(name: &str) -> Store {
        let s = Store::new();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name);
        std::fs::copy(src, s.db()).unwrap();
        s
    }

    pub fn db(&self) -> PathBuf {
        self.dir.join("mem.db")
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(bin());
        c.args(args);
        for (k, _) in std::env::vars() {
            if k.starts_with("MEM_") {
                c.env_remove(k);
            }
        }
        c.env("MEM_DB", self.db()).env("MEM_NO_EMBED", "1");
        c
    }

    pub fn run(&self, args: &[&str]) -> Out {
        self.run_env(args, &[])
    }

    pub fn run_env(&self, args: &[&str], env: &[(&str, &str)]) -> Out {
        let mut c = self.cmd(args);
        for (k, v) in env {
            c.env(k, v);
        }
        let o = c.output().unwrap();
        Out {
            code: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    pub fn run_stdin(&self, args: &[&str], input: &str) -> Out {
        use std::io::Write;
        let mut c = self.cmd(args);
        let mut p = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        p.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let o = p.wait_with_output().unwrap();
        Out {
            code: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// `mem add --project demo ...`, returns the new id (panics when not saved)
    pub fn add(&self, kind: &str, title: &str, summary: &str, extra: &[&str]) -> String {
        let mut a = vec!["add", "--project", "demo", "--kind", kind, "--title", title, "--summary", summary];
        a.extend_from_slice(extra);
        self.run(&a).saved_id()
    }

    pub fn write(&self, name: &str, text: &str) -> String {
        let p = self.dir.join(name);
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().into_owned()
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
