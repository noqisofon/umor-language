//! 実装指示書に記載されたパーサーの受け入れテストケース（テストケース1〜6）。
//!
//! 指示書の例文は分かち書きの空白が省略された形で書かれているため、
//! 送り仮名除去で助詞やキーワードが語幹へ吸収されてしまわないよう、
//! 実際のトークン化結果に沿って空白を補って再現している。

use umor::parser::check_scopes;
use umor::{parse, tokenize, Definition, Expr};

fn parse_src(src: &str) -> umor::Program {
    let tokens = tokenize(src);
    parse(&tokens).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
}

#[test]
fn case1_simple_word_definition() {
    let program = parse_src("挨拶する とは、\n「こんにちは」を　表示すること。");
    assert_eq!(program.definitions.len(), 1);
    let def = &program.definitions[0];
    assert!(def.locals.is_empty());
    assert!(def.variables.is_empty());
    assert_eq!(
        def.body,
        vec![
            Expr::WordCall("「こんにちは」".to_string()),
            Expr::WordCall("を".to_string()),
            Expr::WordCall("表示".to_string()),
        ]
    );
}

#[test]
fn case2_local_word_and_honntai_towa() {
    let program = parse_src(
        "親処理とは\n    子処理とは\n        なにかする\n    本体とは\n        子処理\nこと。",
    );
    assert_eq!(program.definitions.len(), 1);
    let parent = &program.definitions[0];
    assert_eq!(parent.name, "親処理");
    assert!(parent.variables.is_empty());
    assert_eq!(parent.body, vec![Expr::WordCall("子処理".to_string())]);
    assert_eq!(
        parent.locals,
        vec![Definition {
            name: "子処理".to_string(),
            locals: vec![],
            variables: vec![],
            body: vec![Expr::WordCall("なにかする".to_string())],
        }]
    );
}

#[test]
fn case3_if_else_branches() {
    let program = parse_src(
        "判定するとは\n    雨降り？ ならば\n        傘を差す\n    そうでなければ\n        何もしない\n    つぎに\nこと。",
    );
    let def = &program.definitions[0];
    assert_eq!(
        def.body,
        vec![Expr::IfElse {
            cond: vec![Expr::WordCall("雨降り?".to_string())],
            then_branch: vec![Expr::WordCall("傘を差".to_string())],
            else_branch: Some(vec![Expr::WordCall("何".to_string())]),
        }]
    );
}

#[test]
fn case4_variable_declaration() {
    let program = parse_src("カウンターとは\n    Xは 変数\n    0を　X に　いれる\nこと。");
    let def = &program.definitions[0];
    assert_eq!(def.variables, vec!["x".to_string()]);
    assert_eq!(
        def.body,
        vec![
            Expr::NumberLiteral(0),
            Expr::WordCall("を".to_string()),
            Expr::WordCall("x".to_string()),
            Expr::WordCall("に".to_string()),
            Expr::WordCall("いれる".to_string()),
        ]
    );
}

#[test]
fn case5_sibling_local_scope_violation_is_rejected() {
    let program = parse_src(
        "親処理とは\n    子処理１とは\n        Yは 変数\n    子処理２とは\n        Y に　1を　いれる\n    本体とは\n        子処理１\n        子処理２\nこと。",
    );
    let errors = check_scopes(&program).expect_err("兄弟スコープ違反が検出されるはず");
    assert!(errors.iter().any(|e| e.variable == "y"));
}

#[test]
fn case6_subscript_access_desugars_to_no_and_bamme() {
    let program = parse_src("案内するとは\n    売り上げ（1）を　表示する\nこと。");
    let def = &program.definitions[0];
    assert_eq!(
        def.body,
        vec![
            Expr::WordCall("売り上".to_string()),
            Expr::WordCall("の".to_string()),
            Expr::NumberLiteral(1),
            Expr::WordCall("番目".to_string()),
            Expr::WordCall("を".to_string()),
            Expr::WordCall("表示".to_string()),
        ]
    );
}

#[test]
fn case6b_chained_subscript_access_desugars_repeatedly() {
    let program =
        parse_src("案内するとは\n    ダンジョンマップ（X軸座標）（Y座標）を　表示する\nこと。");
    let def = &program.definitions[0];
    assert_eq!(
        def.body,
        vec![
            Expr::WordCall("ダンジョンマップ".to_string()),
            Expr::WordCall("の".to_string()),
            Expr::WordCall("x軸座標".to_string()),
            Expr::WordCall("番目".to_string()),
            Expr::WordCall("の".to_string()),
            Expr::WordCall("y座標".to_string()),
            Expr::WordCall("番目".to_string()),
            Expr::WordCall("を".to_string()),
            Expr::WordCall("表示".to_string()),
        ]
    );
}
