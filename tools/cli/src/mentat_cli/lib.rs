// Copyright 2017 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

#![crate_name = "mentat_cli"]

use std::path::{
    PathBuf,
};

#[macro_use] extern crate failure_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;

extern crate combine;
extern crate env_logger;
extern crate failure;
extern crate getopts;
extern crate linefeed;
extern crate rusqlite;
extern crate tabwriter;
extern crate termion;
extern crate time;

extern crate mentat;
extern crate edn;
extern crate mentat_query;
extern crate mentat_core;
extern crate mentat_db;

use getopts::Options;

use termion::{
    color,
};

static HISTORY_FILE_PATH: &str = ".mentat_history";

/// The Mentat CLI stores input history in a readline-compatible file like "~/.mentat_history".
/// This accords with main other tools which prefix with "." and suffix with "_history": lein,
/// node_repl, python, and sqlite, at least.
pub(crate) fn history_file_path() -> PathBuf {
    let mut p = ::std::env::home_dir().unwrap_or_default();
    p.push(::HISTORY_FILE_PATH);
    p
}

static BLUE: color::Rgb = color::Rgb(0x99, 0xaa, 0xFF);
static GREEN: color::Rgb = color::Rgb(0x77, 0xFF, 0x99);

pub mod command_parser;
pub mod input;
pub mod repl;

#[derive(Debug, Fail)]
pub enum CliError {
    #[fail(display = "{}", _0)]
    CommandParse(String),
}

pub fn run() -> i32 {
    env_logger::init();

    let args = std::env::args().collect::<Vec<_>>();
    let mut opts = Options::new();

    opts.optopt("d", "", "The path to a database to open", "DATABASE");
    if cfg!(feature = "sqlcipher") {
        opts.optopt("k", "key", "The key to use to open the database (only available when using sqlcipher)", "KEY");
    }
    opts.optflag("h", "help", "Print this help message and exit");
    opts.optmulti("q", "query", "Execute a query on startup. Queries are executed after any transacts.", "QUERY");
    opts.optmulti("t", "transact", "Execute a transact on startup. Transacts are executed before queries.", "TRANSACT");
    opts.optmulti("i", "import", "Execute an import on startup. Imports are executed before queries.", "PATH");
    opts.optflag("v", "version", "Print version and exit");
    opts.optflag("", "no-tty", "Don't try to use a TTY for readline-like input processing");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(e) => {
            println!("{}: {}", args[0], e);
            return 1;
        }
    };

    if matches.opt_present("version") {
        print_version();
        return 0;
    }

    if matches.opt_present("help") {
        print_usage(&args[0], &opts);
        return 0;
    }

    // It's still possible to pass this in even if it's not a documented flag above.
    let key = match cfg!(feature = "sqlcipher") {
        true => matches.opt_str("key"),
        false => None,
    };

    let mut last_arg: Option<&str> = None;

    let cmds:Vec<command_parser::Command> = args.iter().filter_map(|arg| {
        match last_arg {
            Some("-d") => {
                last_arg = None;
                if let &Some(ref k) = &key {
                    Some(command_parser::Command::OpenEncrypted(arg.clone(), k.clone()))
                } else {
                    Some(command_parser::Command::Open(arg.clone()))
                }
            },
            Some("-q") => {
                last_arg = None;
                Some(command_parser::Command::Query(arg.clone()))
            },
            Some("-i") => {
                last_arg = None;
                Some(command_parser::Command::Import(arg.clone()))
            },
            Some("-t") => {
                last_arg = None;
                Some(command_parser::Command::Transact(arg.clone()))
            },
            Some(_) |
            None => {
                last_arg = Some(&arg);
                None
            },
        }
    }).collect();

    let mut repl = match repl::Repl::new(!matches.opt_present("no-tty")) {
        Ok(repl) => repl,
        Err(e) => {
            println!("{}", e);
            return 1
        }
    };

    repl.run(Some(cmds));

    0
}

/// Returns a version string.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn print_usage(arg0: &str, opts: &Options) {
    print!("{}", opts.usage(&format!(
        "Usage: {} [OPTIONS] [FILE]", arg0)));
}

fn print_version() {
    println!("mentat {}", version());
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
