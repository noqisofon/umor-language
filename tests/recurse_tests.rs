//! 実装指示書（issue #13、ADR-0009）に記載された`再帰`構文キーワードの
//! 受け入れテストケース（テストケース1〜5）。

use std::cell::RefCell;
use std::rc::Rc;
use umor::parser::check_scopes;
use umor::{parse, tokenize, Expr, Interpreter, Value};

fn parse_src(src: &str) -> umor::Program {
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

#[test]
fn parser_recognizes_recurse_keyword_as_a_dedicated_ast_node() {
    let program = parse_src(
        "階乗 とは\n    複製 1 等しい？ ならば\n        捨 1\n    そうでなければ\n        複製 1 引く 再帰する 掛ける\n    つぎに\nこと。",
    );
    let def = &program.definitions[0];
    match &def.body[0] {
        Expr::IfElse { else_branch, .. } => {
            let else_branch = else_branch.as_ref().expect("そうでなければ節があるはず");
            assert!(
                else_branch.contains(&Expr::SelfRecurse),
                "else節に SelfRecurse が含まれるはず: {else_branch:?}"
            );
            assert!(
                !else_branch.contains(&Expr::WordCall("階乗".to_string())),
                "「再帰」は辞書引き対象のWordCallになってはいけない: {else_branch:?}"
            );
        }
        other => panic!("expected IfElse, got {other:?}"),
    }
}

/// テスト1：基本の自己再帰（階乗）。
#[test]
fn test1_basic_self_recursion_factorial() {
    let program = parse_src(
        "階乗 とは\n    複製 1 等しい？ ならば\n        捨 1\n    そうでなければ\n        複製 1 引く 再帰する 掛ける\n    つぎに\nこと。",
    );
    let mut interp = Interpreter::new();
    interp.load_program(&program);

    interp.push_value(Value::Number(5));
    interp.run_word("階乗").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 1);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(120));
}

/// テスト2：トップレベルでの`再帰`は構文エラー。
#[test]
fn test2_recurse_at_top_level_is_a_syntax_error() {
    let tokens = tokenize("再帰する。").unwrap();
    let err = parse(&tokens).expect_err("トップレベルの「再帰」は構文エラーになるはず");
    assert!(err.message.contains("再帰"), "got message: {}", err.message);
}

/// テスト3：`再帰`を処理単語名として定義しようとするとエラー。
#[test]
fn test3_defining_a_word_named_recurse_is_a_syntax_error() {
    let tokens = tokenize("再帰 とは、「なにか」と 表示する こと。").unwrap();
    let err = parse(&tokens).expect_err("「再帰」を処理単語名にすることは構文エラーになるはず");
    assert!(err.message.contains("再帰"), "got message: {}", err.message);
}

/// テスト4：局所処理単語内の`再帰`は局所処理単語自身を指す（親処理単語ではない）。
#[test]
fn test4_recurse_inside_a_local_word_targets_the_local_itself() {
    let program = parse_src(
        "親処理 とは\n    局所処理 とは\n        複製 0 等しい？ ならば\n            捨\n        そうでなければ\n            「呼ばれた」を 表示する\n            複製 1 引く 再帰する 捨\n        つぎに\n    本体 とは\n        3 局所処理\n    こと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("親処理").expect("実行に失敗した");

    assert_eq!(
        *log.borrow(),
        vec!["呼ばれた".to_string(); 3],
        "局所処理の再帰は3回（n=3,2,1）呼ばれ、親処理を呼び直してはいけない"
    );
}

/// テスト5：ADR-0008（再定義イディオム）とADR-0009（`再帰`）の組み合わせ。
/// 2行目の「挨拶する」は`再帰`ではないので、ADR-0008により1行目の
/// （終端する・無限にならない）定義を指す。
#[test]
fn test5_redefinition_idiom_and_recurse_are_independent() {
    let program = parse_src(
        "挨拶 とは、「こんにちは」と 表示する こと。\n挨拶 とは、「やあ」と 表示して 挨拶する こと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp
        .run_word("挨拶")
        .expect("実行に失敗した（無限再帰になっていないか確認）");

    assert_eq!(
        *log.borrow(),
        vec!["やあ".to_string(), "こんにちは".to_string()]
    );
}

#[test]
fn scope_check_ignores_self_recurse_node() {
    let program = parse_src(
        "階乗 とは\n    複製 1 等しい？ ならば\n        捨 1\n    そうでなければ\n        複製 1 引く 再帰する 掛ける\n    つぎに\nこと。",
    );
    assert_eq!(check_scopes(&program), Ok(()));
}
