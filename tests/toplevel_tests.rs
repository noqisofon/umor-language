//! issue #7: トップレベル即時実行式・ファイル実行・REPLの受け入れテスト。

use std::cell::RefCell;
use std::rc::Rc;
use umor::{parse_top_level_item, run_source, tokenize, Expr, Interpreter, TopLevelItem, Value};

/// テストダブル: `表示`を、標準出力の代わりに`log`へ蓄積するよう差し替える。
fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}

#[test]
fn case1_top_level_expr_executes_immediately() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, "「こんにちは、世界！」を　表示する。").expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["こんにちは、世界！".to_string()]);
}

#[test]
fn case2_definitions_and_top_level_exprs_interleave_in_order() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "「開始」を　表示する。\n\n挨拶する とは、\n    「こんにちは」を　表示する\nこと。\n\n挨拶する。\n\n「終了」を　表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(
        *log.borrow(),
        vec![
            "開始".to_string(),
            "こんにちは".to_string(),
            "終了".to_string(),
        ]
    );
}

#[test]
fn word_definition_alone_registers_but_does_not_run() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(
        &mut interp,
        "挨拶する とは、\n    「こんにちは」を　表示する\nこと。",
    )
    .expect("実行に失敗した");

    assert!(log.borrow().is_empty());
}

#[test]
fn case3_stray_period_inside_definition_body_is_a_parse_error() {
    let mut interp = Interpreter::new();
    let err = run_source(
        &mut interp,
        "挨拶する とは、「こんにちは」を　表示する。 こと。",
    )
    .expect_err("本体途中の「。」は構文エラーになるはず");
    match err {
        umor::UmorError::Parse(e) => assert!(e.message.contains("こと。"), "got {e}"),
        other => panic!("expected Parse error, got {other}"),
    }
}

#[test]
fn period_used_as_a_string_literal_content_is_unaffected() {
    // 文字列リテラル内の「。」は構造上の区切り記号ではないので、影響を受けない。
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, "「こんにちは。」を　表示する。").expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["こんにちは。".to_string()]);
}

#[test]
fn stray_period_inside_if_else_branch_is_a_parse_error() {
    let tokens = tokenize(
        "判定する とは\n    雨降り？ ならば\n        傘を差す。\n    そうでなければ\n        何もしない\n    つぎに\nこと。",
    )
    .unwrap();
    let mut pos = 0usize;
    assert!(parse_top_level_item(&tokens, &mut pos).is_err());
}

#[test]
fn multiple_top_level_statements_on_one_line_run_in_order() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, "5　3　加　表示する。　「次」を　表示する。").expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["8".to_string(), "次".to_string()]);
}

#[test]
fn undefined_word_at_top_level_is_a_runtime_error_reported_via_run_source() {
    let mut interp = Interpreter::new();
    let err = run_source(&mut interp, "ぞんざいするワード。").expect_err("実行時エラーになるはず");
    match err {
        umor::UmorError::Runtime(report) => {
            assert_eq!(
                report.error,
                umor::RuntimeError::UndefinedWord("ぞんざいするワード".to_string())
            );
        }
        other => panic!("expected Runtime error, got {other}"),
    }
}

#[test]
fn parse_top_level_item_distinguishes_definition_from_expr() {
    let tokens = tokenize("挨拶する とは、\n    「こんにちは」を　表示する\nこと。").unwrap();
    let mut pos = 0usize;
    match parse_top_level_item(&tokens, &mut pos).unwrap() {
        TopLevelItem::Definition(def) => assert_eq!(def.name, "挨拶".to_string()),
        other => panic!("expected Definition, got {other:?}"),
    }
    assert_eq!(pos, tokens.len());

    let tokens = tokenize("「こんにちは」を　表示する。").unwrap();
    let mut pos = 0usize;
    match parse_top_level_item(&tokens, &mut pos).unwrap() {
        TopLevelItem::Expr(exprs) => {
            assert_eq!(
                exprs,
                vec![
                    Expr::WordCall("「こんにちは」".to_string()),
                    Expr::WordCall("を".to_string()),
                    Expr::WordCall("表示".to_string()),
                ]
            );
        }
        other => panic!("expected Expr, got {other:?}"),
    }
    assert_eq!(pos, tokens.len());
}

#[test]
fn value_display_round_trips_through_run_source() {
    let mut interp = Interpreter::new();
    run_source(&mut interp, "1　2　加。").expect("実行に失敗した");
    assert_eq!(interp.pop_value().unwrap(), Value::Number(3));
}
