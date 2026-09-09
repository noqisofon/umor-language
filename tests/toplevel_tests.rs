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
                    Expr::StringLiteral("こんにちは".to_string()),
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

// ADR-0026: トップレベルの`Xは 変数`宣言をグローバル変数として機能させる。

#[test]
fn adr0026_parse_top_level_item_recognizes_variable_decl() {
    let tokens = tokenize("合言葉 は 変数。").unwrap();
    let mut pos = 0usize;
    match parse_top_level_item(&tokens, &mut pos).unwrap() {
        TopLevelItem::Expr(exprs) => {
            assert_eq!(exprs, vec![Expr::VariableDecl("合言葉".to_string())]);
        }
        other => panic!("expected Expr, got {other:?}"),
    }
    assert_eq!(pos, tokens.len());
}

#[test]
fn adr0026_top_level_variable_is_usable_from_top_level_exprs() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "合言葉 は 変数。\n\n「のばら」を　合言葉に　入れる。\n\n合言葉を　読んで　表示する。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["のばら".to_string()]);
}

#[test]
fn adr0026_top_level_variable_is_visible_from_word_definitions() {
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    install_logging_display(&mut interp, log.clone());

    let src = "合言葉 は 変数。\n\n「のばら」を　合言葉に　入れる。\n\n挨拶 とは\n    合言葉を　読んで　表示する\nこと。\n\n挨拶。";
    run_source(&mut interp, src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["のばら".to_string()]);
}

// ADR-0030: `〈新語〉も 〈既存語〉の 別名` によるワードエイリアス宣言。

#[test]
fn adr0030_parse_top_level_item_recognizes_alias_decl() {
    // 「も」「の」はいずれも構造キーワードのため、直前にスペースが必要
    // （ADR-0015の「は」と同様、スペースがないと送り仮名として吸収され消える）。
    let tokens = tokenize("交換 も 取替 の 別名。").unwrap();
    let mut pos = 0usize;
    match parse_top_level_item(&tokens, &mut pos).unwrap() {
        TopLevelItem::Expr(exprs) => {
            assert_eq!(
                exprs,
                vec![Expr::AliasDecl {
                    new_name: "交換".to_string(),
                    existing_name: "取替".to_string(),
                }]
            );
        }
        other => panic!("expected Expr, got {other:?}"),
    }
    assert_eq!(pos, tokens.len());
}

#[test]
fn adr0030_alias_dispatches_to_the_same_implementation_as_the_existing_word() {
    // 既存ワード「加」に別名「add2」を宣言すると、「add2」からも同じ動作をする
    // （送り仮名正規化のブレを避けるため、別名にはASCII識別子を使う）。
    let mut interp = Interpreter::new();
    run_source(&mut interp, "add2 も 加 の 別名。").expect("実行に失敗した");

    run_source(&mut interp, "1　2　add2。").expect("実行に失敗した");
    assert_eq!(interp.pop_value().unwrap(), Value::Number(3));
}

#[test]
fn adr0030_alias_to_an_undefined_word_errors_only_when_actually_called() {
    // 宣言時点では存在チェックをしない。呼び出し時に初めてUndefinedWordになる。
    let mut interp = Interpreter::new();
    run_source(&mut interp, "missing1 も missing2 の 別名。").expect("宣言自体は成功するはず");

    let err = run_source(&mut interp, "missing1。").expect_err("実行時エラーになるはず");
    match err {
        umor::UmorError::Runtime(report) => {
            assert_eq!(
                report.error,
                umor::RuntimeError::UndefinedWord("missing2".to_string())
            );
        }
        other => panic!("expected Runtime error, got {other}"),
    }
}

#[test]
fn adr0026_redeclaring_a_top_level_variable_resets_it_to_uninitialized() {
    let mut interp = Interpreter::new();
    let src = "X は 変数。\n\n1を　Xに　入れる。\n\nX は 変数。\n\nXを　読む。";
    let err = run_source(&mut interp, src).unwrap_err();
    assert!(err.to_string().contains("まだ値が代入されていません"));
}
