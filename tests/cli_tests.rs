//! issue #7 ケース4・ケース5: ファイル実行CLI・REPLの受け入れテスト。
//!
//! ビルド済みバイナリ（`umor`）をサブプロセスとして起動し、標準入出力を
//! 検証する。

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_umor")
}

#[test]
fn case4_file_execution_runs_top_level_statements_in_order() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("umor_cli_test_{}.umor", std::process::id()));
    std::fs::write(
        &path,
        "「開始」を　表示する。\n\n挨拶する とは、\n    「こんにちは」を　表示する\nこと。\n\n挨拶する。\n\n「終了」を　表示する。",
    )
    .unwrap();

    let output = Command::new(bin())
        .arg(&path)
        .output()
        .expect("umorバイナリの実行に失敗した");

    std::fs::remove_file(&path).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["開始", "こんにちは", "終了"]);
}

#[test]
fn file_execution_reports_error_and_exits_nonzero_on_parse_error() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("umor_cli_test_err_{}.umor", std::process::id()));
    std::fs::write(&path, "挨拶する とは、「こんにちは」を　表示する。 こと。").unwrap();

    let output = Command::new(bin())
        .arg(&path)
        .output()
        .expect("umorバイナリの実行に失敗した");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn case5_repl_executes_each_line_immediately() {
    let mut child = Command::new(bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("umorバイナリの起動に失敗した");

    // 明示的に`stdin`をクローズ（EOF）しないと、REPLループが終了せず
    // `wait_with_output`がハングしてしまう。
    let mut stdin = child.stdin.take().expect("stdinを取得できなかった");
    writeln!(stdin, "「こんにちは」を　表示する。").unwrap();
    writeln!(stdin, "挨拶する とは、「こんにちは」を　表示する こと。").unwrap();
    writeln!(stdin, "挨拶する。").unwrap();
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("umorバイナリの終了待機に失敗した");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // プロンプト（"umor> "）を取り除いた各行のうち、空でないものが
    // 期待した表示結果の順で現れることを確認する。
    let shown: Vec<String> = stdout
        .lines()
        .map(|l| l.trim_start_matches("umor> ").to_string())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        shown,
        vec!["こんにちは".to_string(), "こんにちは".to_string()]
    );
}
