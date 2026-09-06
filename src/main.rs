use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

use umor::{run_source, Interpreter};

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1) {
        Some(path) => run_file(path),
        None => run_repl(),
    }
}

/// `.umor`ファイルを読み込み、`run_source`で実行する。
/// トップレベル式がその場で実行されるため、実行したい処理はファイルに
/// そのまま書けばよい（どのワードを呼ぶかを別途指定する必要はない）。
fn run_file(path: &str) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ファイルを読み込めません: {path}: {e}");
            std::process::exit(1);
        }
    };

    let mut interp = Interpreter::new();
    if let Err(e) = run_source(&mut interp, &src) {
        eprintln!("エラー: {e}");
        std::process::exit(1);
    }
}

/// 標準入力から1行ずつ読み込み、`run_source`と同じコアロジックで処理する。
fn run_repl() {
    let mut interp = Interpreter::new();
    let stdin = io::stdin();
    print!("umor> ");
    io::stdout().flush().ok();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            print!("umor> ");
            io::stdout().flush().ok();
            continue;
        }
        if let Err(e) = run_source(&mut interp, &line) {
            eprintln!("エラー: {e}");
        }
        print!("umor> ");
        io::stdout().flush().ok();
    }
}
