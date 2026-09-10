//! ADR-0013: `check_scopes`（静的スコープチェック）のファイル実行への
//! 組み込み（`run_file_source`）の受け入れテスト。
//!
//! `run_file_source`はファイル全体を先にパースし切ってから`check_scopes`を
//! 通し、違反があれば一切評価に入らずに`UmorError::Scope`を返す。既存の
//! `run_source`（REPL・`含める`／`必要`共通コア）はこのスコープ外であり、
//! 今回の変更で意図せず変わっていないことも併せて確認する。

use std::cell::RefCell;
use std::rc::Rc;
use umor::{run_file_source, run_source, Interpreter, UmorError};

/// テストダブル: `表示`を、標準出力の代わりに`log`へ蓄積するよう差し替える。
fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}

const SIBLING_VIOLATION_SRC: &str = "親処理 とは\n    子処理１ とは\n        カウンタ は 変数\n    子処理２ とは\n        カウンタを　読んで　表示する\n    本体 とは\n        子処理１\n        子処理２\nこと。\n\n親処理。";

#[test]
fn sibling_scope_violation_is_caught_before_any_execution() {
    let mut interp = Interpreter::new();
    let err = run_file_source(&mut interp, SIBLING_VIOLATION_SRC)
        .expect_err("兄弟局所処理単語からの変数アクセスはスコープ違反になるはず");

    match err {
        UmorError::Scope(errors) => {
            assert!(
                errors.iter().any(|e| e.variable == "カウンタ"),
                "got {errors:?}"
            );
        }
        other => panic!("expected UmorError::Scope, got {other}"),
    }
}

#[test]
fn multiple_scope_violations_are_all_reported_together() {
    let src = "親処理 とは\n    子処理１ とは\n        カウンタ は 変数\n        合計 は 変数\n    子処理２ とは\n        カウンタを　読んで　表示する\n        合計を　読んで　表示する\n    本体 とは\n        子処理１\n        子処理２\nこと。\n\n親処理。";

    let mut interp = Interpreter::new();
    let err = run_file_source(&mut interp, src).expect_err("複数のスコープ違反が報告されるはず");

    match err {
        UmorError::Scope(errors) => {
            assert_eq!(errors.len(), 2, "got {errors:?}");
            assert!(errors.iter().any(|e| e.variable == "カウンタ"));
            assert!(errors.iter().any(|e| e.variable == "合計"));
        }
        other => panic!("expected UmorError::Scope, got {other}"),
    }
}

#[test]
fn scope_violation_prevents_any_execution_including_side_effects() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!("「開始」を　表示する。\n\n{SIBLING_VIOLATION_SRC}");
    let err = run_file_source(&mut interp, &src)
        .expect_err("スコープ違反があるファイルは実行前にエラーになるはず");

    assert!(matches!(err, UmorError::Scope(_)), "got {err}");
    assert!(
        log.borrow().is_empty(),
        "スコープ違反時はトップレベルの正常な部分も含めて一切実行されないはず: {:?}",
        log.borrow()
    );
}

#[test]
fn parent_to_child_access_is_not_a_scope_violation() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "親処理 とは\n    カウンタ は 変数\n    子処理 とは\n        1を　カウンタに　入れる\n        カウンタを　読んで　表示する\n    本体 とは\n        子処理\nこと。\n\n親処理。";
    run_file_source(&mut interp, src).expect("親で宣言した変数を子から読み書きするのは正当");

    assert_eq!(*log.borrow(), vec!["1".to_string()]);
}

#[test]
fn run_source_is_unaffected_and_does_not_perform_scope_checking() {
    let mut interp = Interpreter::new();
    // `run_source`（既存の逐次パース・逐次実行コア）は今回のスコープ外
    // であり、スコープチェックを組み込んでいない。同じ違反コードを渡しても
    // `UmorError::Scope`にはならない（未定義ワードとしての実行時エラーになる）。
    let err = run_source(&mut interp, SIBLING_VIOLATION_SRC)
        .expect_err("兄弟からの未定義変数アクセスは実行時エラーになるはず");

    assert!(
        !matches!(err, UmorError::Scope(_)),
        "run_sourceはスコープチェックを行わないはず: got {err}"
    );
}
