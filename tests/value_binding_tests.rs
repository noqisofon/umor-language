//! impl-0033: `可変値`（value）・`定数値`（constant）の受け入れテスト。
//! ADR-0020（宣言構文）・ADR-0031（`可変値`への`代入`部分）に対応する。

use std::cell::RefCell;
use std::rc::Rc;
use umor::{parse, run_source, tokenize, Interpreter, RuntimeError, UmorError, Value};

fn build(src: &str) -> umor::Program {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
}

/// テストダブル: `表示`を、標準出力の代わりに`log`へ蓄積するよう差し替える。
fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}

fn runtime_error(err: UmorError) -> RuntimeError {
    match err {
        UmorError::Runtime(report) => report.error,
        other => panic!("expected Runtime error, got {other}"),
    }
}

#[test]
fn kahenchi_declaration_pushes_its_current_value_when_named() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "りんご は 可変値で 10 が 初期値。\n\nりんご を 表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["10".to_string()]);
}

#[test]
fn kahenchi_can_be_reassigned_with_dainyu() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "りんご は 可変値で 10 が 初期値。\n\n20 りんご 代入。\n\nりんご を 表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["20".to_string()]);
}

#[test]
fn teisuuchi_cannot_be_reassigned() {
    let mut interp = Interpreter::new();

    let src = "りんご は 定数値で 10 が 初期値。\n\n20 りんご 代入。";
    let err = run_source(&mut interp, src).expect_err("定数値への代入はエラーになるはず");

    assert_eq!(
        runtime_error(err),
        RuntimeError::AssignToConstant("りんご".to_string())
    );
}

#[test]
fn dainyu_to_an_undeclared_name_is_undefined_word_error() {
    let mut interp = Interpreter::new();

    let src = "20 みかん 代入。";
    let err = run_source(&mut interp, src).expect_err("未宣言の名前への代入はエラーになるはず");

    assert_eq!(
        runtime_error(err),
        RuntimeError::UndefinedWord("みかん".to_string())
    );
}

#[test]
fn kahenchi_declared_in_parent_is_visible_and_mutable_from_child() {
    let program = build(
        "親処理 とは\n    カウンタ は 可変値で 0 が 初期値\n    子処理 とは\n        カウンタ を 表示する\n        99 カウンタ 代入\n    本体 とは\n        子処理\n        カウンタ を 表示する\nこと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("親処理").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["0".to_string(), "99".to_string()]);
}

#[test]
fn kahenchi_declared_in_one_local_word_is_invisible_from_a_sibling() {
    let program = build(
        "親処理 とは\n    子処理１ とは\n        カウンタ は 可変値で 1 が 初期値\n    子処理２ とは\n        カウンタ を 表示する\n    本体 とは\n        子処理１\n        子処理２\nこと。",
    );
    let mut interp = Interpreter::new();
    interp.load_program(&program);

    let err = interp
        .run_word("親処理")
        .expect_err("兄弟局所処理単語からの可変値アクセスはエラーになるはず");
    assert_eq!(err, RuntimeError::UndefinedWord("カウンタ".to_string()));
}

#[test]
fn kahenchi_init_expr_can_be_a_multi_word_expression() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "りんご は 可変値で 1 2 加 が 初期値。\n\nりんご を 表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["3".to_string()]);
}

#[test]
fn kahenchi_redeclaration_creates_a_fresh_slot() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "りんご は 可変値で 1 が 初期値。\n\nりんご は 可変値で 2 が 初期値。\n\nりんご を 表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["2".to_string()]);
}

#[test]
fn value_is_pushed_as_a_plain_value_not_a_var_ref() {
    // `変数`とは異なり、`可変値`は名前を書いた時点でアドレス（`VarRef`）
    // ではなく値そのものが積まれるため、`読`を挟まずそのまま算術演算に使える。
    let mut interp = Interpreter::new();

    let src = "りんご は 可変値で 10 が 初期値。\n\nりんご 5 加。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(interp.pop_value().unwrap(), Value::Number(15));
}
