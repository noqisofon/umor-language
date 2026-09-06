//! 実装指示書に記載された評価器の受け入れテストケース（テストケース1〜7）。

use std::cell::RefCell;
use std::rc::Rc;
use umor::{parse, tokenize, Interpreter, RuntimeError, Value};

fn build(src: &str) -> umor::Program {
    let tokens = tokenize(src).unwrap();
    parse(&tokens).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
}

#[test]
fn case1_basic_word_combination() {
    // にゃんぷっぷー？ とは 複製 ぶろぶ？ 取替 猫っぽい？ かつ こと。
    // 送り仮名除去による正規化のブレを避けるため、ダミー述語の名前には
    // ASCII識別子を使う（意味は指示書どおり: ( x -- bool )、常にtrueを返す）。
    let program = build("test1 とは\n    複製 pred1 取替 pred2 かつ\nこと。");
    let mut interp = Interpreter::new();
    interp.register_native("pred1", |interp| {
        interp.pop_value()?;
        interp.push_value(Value::Bool(true));
        Ok(())
    });
    interp.register_native("pred2", |interp| {
        interp.pop_value()?;
        interp.push_value(Value::Bool(true));
        Ok(())
    });
    interp.load_program(&program);

    interp.push_value(Value::Number(0)); // 複製の対象となる、なんでもいい初期値
    interp.run_word("test1").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 1);
    assert_eq!(interp.pop_value().unwrap(), Value::Bool(true));
}

#[test]
fn case2_if_else_true_branch() {
    let program = build(
        "hantei とは\n    ame ならば\n        「傘を持って行く」を 表示する\n    そうでなければ\n        「傘は不要」を 表示する\n    つぎに\nこと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    interp.register_native("ame", |interp| {
        interp.push_value(Value::Bool(true));
        Ok(())
    });
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("hantei").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["傘を持って行く".to_string()]);
}

#[test]
fn case2_if_else_false_branch() {
    let program = build(
        "hantei とは\n    ame ならば\n        「傘を持って行く」を 表示する\n    そうでなければ\n        「傘は不要」を 表示する\n    つぎに\nこと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    interp.register_native("ame", |interp| {
        interp.push_value(Value::Bool(false));
        Ok(())
    });
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("hantei").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["傘は不要".to_string()]);
}

#[test]
fn case3_variable_declaration_assignment_and_read() {
    // 「入れる」（送り仮名除去後は「入」）で代入し、「読む」（同「読」）で
    // 変数参照を明示的に現在値へ解決してから表示する。
    let program =
        build("カウンターとは\n    Xは 変数\n    0を　X に　入れる\n    X 読む 表示する\nこと。");
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("カウンター").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["0".to_string()]);
}

#[test]
fn case3b_displaying_a_variable_without_read_shows_its_var_ref_representation() {
    // 「読」を挟まずに変数をそのまま`表示`すると、値そのものではなく
    // 変数参照であることが分かる文字列表現が出力される（Phase 1の仕様）。
    let program =
        build("カウンターとは\n    Xは 変数\n    0を　X に　入れる\n    X 表示する\nこと。");
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("カウンター").expect("実行に失敗した");
    assert_eq!(log.borrow().len(), 1);
    let shown = &log.borrow()[0];
    assert!(shown.contains('x') && shown.contains('0'), "got {shown:?}");
    assert_ne!(shown, "0");
}

#[test]
fn case3c_arithmetic_on_an_unread_variable_is_a_type_error() {
    // Phase 1では「読」を挟まない限り、変数参照は具体的な値として扱えない。
    let program = build("たすとは\n    Xは 変数\n    1を　X に　入れる\n    X 1 加\nこと。");
    let mut interp = Interpreter::new();
    interp.load_program(&program);

    let err = interp
        .run_word("たす")
        .expect_err("未解決の変数参照での算術演算はエラーになるはず");
    assert!(
        matches!(err, RuntimeError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn case4_local_word_sees_parent_variable() {
    let program = build(
        "親処理とは\n    子処理とは\n        X 読む 表示する\n    本体とは\n        Xは 変数\n        42 を　X に　入れる\n        子処理\nこと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("親処理").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["42".to_string()]);
}

#[test]
fn case5_sibling_local_variable_is_reported_as_undefined() {
    // パーサー側の静的スコープチェック（check_scopes）を意図的に呼ばずに実行し、
    // 評価器が兄弟局所処理単語の変数へのアクセスを「未定義ワード」として
    // 検出することを確認する。
    let program = build(
        "親処理とは\n    子処理１とは\n        Yは 変数\n    子処理２とは\n        Y に　1を　入れる\n    本体とは\n        子処理１\n        子処理２\nこと。",
    );
    let mut interp = Interpreter::new();
    interp.load_program(&program);

    let err = interp
        .run_word("親処理")
        .expect_err("兄弟スコープ違反はエラーになるはず");
    assert_eq!(err, RuntimeError::UndefinedWord("y".to_string()));
}

#[test]
fn case6_subscript_access_via_bamme() {
    let program = build(
        "配列作成とは\n    空配列\n    10　追加\n    20　追加\n    30　追加\nこと。\nテストとは\n    配列作成\n    0　番目\n    表示する\nこと。",
    );
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let mut interp = Interpreter::new();
    install_logging_display(&mut interp, log.clone());
    interp.load_program(&program);

    interp.run_word("テスト").expect("実行に失敗した");
    assert_eq!(*log.borrow(), vec!["10".to_string()]);
}

#[test]
fn case7_undefined_word_is_a_runtime_error() {
    let program = build("存在しないワードを呼ぶとは\n    ぞんざいするワード\nこと。");
    let entry_name = program.definitions[0].name.clone();
    let called_name = match &program.definitions[0].body[0] {
        umor::Expr::WordCall(name) => name.clone(),
        other => panic!("expected WordCall, got {other:?}"),
    };

    let mut interp = Interpreter::new();
    interp.load_program(&program);

    let err = interp
        .run_word(&entry_name)
        .expect_err("未定義ワードはエラーになるはず");
    assert_eq!(err, RuntimeError::UndefinedWord(called_name));
}

/// テストダブル: `表示`を、標準出力の代わりに`log`へ蓄積するよう差し替える。
fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}
