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
        build("カウンター とは\n    X は 変数\n    0を　X に　入れる\n    X 読む 表示する\nこと。");
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
        build("カウンター とは\n    X は 変数\n    0を　X に　入れる\n    X 表示する\nこと。");
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
    let program = build("たす とは\n    X は 変数\n    1を　X に　入れる\n    X 1 加\nこと。");
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
        "親処理 とは\n    子処理 とは\n        X 読む 表示する\n    本体 とは\n        X は 変数\n        42 を　X に　入れる\n        子処理\nこと。",
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
        "親処理 とは\n    子処理１ とは\n        Y は 変数\n    子処理２ とは\n        Y に　1を　入れる\n    本体 とは\n        子処理１\n        子処理２\nこと。",
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
        "配列作成 とは\n    空配列\n    10　追加\n    20　追加\n    30　追加\nこと。\nテスト とは\n    配列作成\n    0　番目\n    表示する\nこと。",
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
    let program = build("存在しないワードを呼ぶ とは\n    ぞんざいするワード\nこと。");
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

#[test]
fn adr0017_rotation_word_rotates_top_three_stack_items() {
    // 回転（Forthの`rot`相当）: ( a b c -- b c a )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.push_value(Value::Number(3));
    interp.run_word("回転").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 3);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(3));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
}

#[test]
fn adr0017_remainder_word_computes_modulo() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(7));
    interp.push_value(Value::Number(3));
    interp.run_word("余").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 1);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
}

#[test]
fn adr0017_rotation_and_remainder_words_normalize_through_okurigana_stripping() {
    // 「回転する」のような活用形でも、送り仮名除去により
    // 辞書登録済みの語幹（「回転」「余」）へ正規化されて呼び出せる。
    let program =
        build("テスト とは\n    1 2 3 回転する\n    捨てる 捨てる\n    10 3 余り\nこと。");
    let mut interp = Interpreter::new();
    interp.load_program(&program);

    interp.run_word("テスト").expect("実行に失敗した");

    // 「1 2 3 回転する」で ( 1 2 3 -- 2 3 1 )、続く「捨てる 捨てる」で
    // 上2つ（1, 3）を捨て、残るのは最初にrotされて底へ回った「2」のみ。
    // 続けて「10 3 余り」で 10 % 3 = 1 が積まれる。
    assert_eq!(interp.stack_len(), 2);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
}

#[test]
fn adr0017_remainder_by_zero_is_a_runtime_error() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(7));
    interp.push_value(Value::Number(0));

    let err = interp
        .run_word("余")
        .expect_err("0での剰余はエラーになるはず");
    assert_eq!(err, RuntimeError::DivisionByZero);
}

// ADR-0029: フェーズ1COREワードセット拡充（9ワード追加＋交換/取替統合）。

#[test]
fn adr0029_depth_word_pushes_current_stack_length() {
    // `run_word`はトークナイザーの正規化を経由しないため、送り仮名除去後の
    // 語幹（「深さ」→「深」）を直接指定する（「入」「読」と同様の理由）。
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(10));
    interp.push_value(Value::Number(20));
    interp.run_word("深").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 3);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
}

#[test]
fn adr0029_two_drop_word_discards_top_two_items() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.push_value(Value::Number(3));
    interp.run_word("二捨").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 1);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
}

#[test]
fn adr0029_two_dup_word_duplicates_top_two_items() {
    // ( a b -- a b a b )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.run_word("二複製").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 4);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
}

#[test]
fn adr0029_two_swap_word_swaps_top_two_pairs() {
    // ( a b c d -- c d a b )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.push_value(Value::Number(3));
    interp.push_value(Value::Number(4));
    interp.run_word("二交換").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 4);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(4));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(3));
}

#[test]
fn adr0029_nip_word_drops_the_second_item() {
    // ( a b -- b )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.run_word("取替捨").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 1);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
}

#[test]
fn adr0029_tuck_word_inserts_top_below_second_item() {
    // ( a b -- b a b )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.run_word("取替越").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 3);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(1));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(2));
}

#[test]
fn adr0029_pick_word_duplicates_the_nth_item_from_top() {
    // ( 10 20 30 1 摘み -- 10 20 30 20 )
    // `run_word`は送り仮名除去後の語幹「摘」を直接指定する（「深」と同様の理由）。
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(10));
    interp.push_value(Value::Number(20));
    interp.push_value(Value::Number(30));
    interp.push_value(Value::Number(1));
    interp.run_word("摘").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 4);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(20));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(30));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(20));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(10));
}

#[test]
fn adr0029_pick_zero_is_equivalent_to_dup() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(42));
    interp.push_value(Value::Number(0));
    interp.run_word("摘").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 2);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(42));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(42));
}

#[test]
fn adr0029_pick_beyond_stack_depth_is_a_stack_underflow_error() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(10));
    interp.push_value(Value::Number(5)); // 深さ1のスタックに対し、添字5は範囲外

    let err = interp
        .run_word("摘")
        .expect_err("スタックの深さを超えた摘みはエラーになるはず");
    assert_eq!(err, RuntimeError::StackUnderflow);
}

#[test]
fn adr0029_roll_word_moves_the_nth_item_to_top() {
    // ( 10 20 30 2 転 -- 20 30 10 )
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(10));
    interp.push_value(Value::Number(20));
    interp.push_value(Value::Number(30));
    interp.push_value(Value::Number(2));
    interp.run_word("転").expect("実行に失敗した");

    assert_eq!(interp.stack_len(), 3);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(10));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(30));
    assert_eq!(interp.pop_value().unwrap(), Value::Number(20));
}

#[test]
fn adr0029_roll_beyond_stack_depth_is_a_stack_underflow_error() {
    let mut interp = Interpreter::new();
    interp.push_value(Value::Number(10));
    interp.push_value(Value::Number(5)); // 深さ1のスタックに対し、添字5は範囲外

    let err = interp
        .run_word("転")
        .expect_err("スタックの深さを超えた転はエラーになるはず");
    assert_eq!(err, RuntimeError::StackUnderflow);
}

#[test]
fn adr0029_stack_display_word_shows_contents_bottom_to_top_without_consuming() {
    let sink = Rc::new(RefCell::new(umor::BufferSink::new()));
    let mut interp = Interpreter::with_output(Box::new(sink.clone()));
    interp.push_value(Value::Number(1));
    interp.push_value(Value::Number(2));
    interp.push_value(Value::Number(3));
    interp.run_word("スタック表示").expect("実行に失敗した");

    assert_eq!(sink.borrow().contents(), "1 2 3\n");
    // スタックを消費していないことを確認する。
    assert_eq!(interp.stack_len(), 3);
    assert_eq!(interp.pop_value().unwrap(), Value::Number(3));
}

#[test]
fn adr0029_kokan_and_torikae_produce_the_same_result() {
    // 交換は取替の別名（ADR-0030）。同じ入力に対し同じ結果になることを確認する。
    let mut interp_kokan = Interpreter::new();
    interp_kokan.push_value(Value::Number(1));
    interp_kokan.push_value(Value::Number(2));
    interp_kokan.run_word("交換").expect("実行に失敗した");

    let mut interp_torikae = Interpreter::new();
    interp_torikae.push_value(Value::Number(1));
    interp_torikae.push_value(Value::Number(2));
    interp_torikae.run_word("取替").expect("実行に失敗した");

    assert_eq!(interp_kokan.stack_len(), interp_torikae.stack_len());
    assert_eq!(
        interp_kokan.pop_value().unwrap(),
        interp_torikae.pop_value().unwrap()
    );
    assert_eq!(
        interp_kokan.pop_value().unwrap(),
        interp_torikae.pop_value().unwrap()
    );
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
fn adr0028_buffer_sink_accumulates_display_output() {
    // ADR-0028: `Interpreter::with_output`に`BufferSink`を注入すると、
    // `表示`ワードの出力が標準出力の代わりにバッファへ蓄積される。
    // `Rc<RefCell<BufferSink>>`をクローンして持っておくことで、実行後に
    // インタプリタの外から蓄積内容を確認できる。
    let program =
        build("テスト とは\n    「こんにちは」を　表示する\n    「世界」を　表示する\nこと。");
    let sink = Rc::new(RefCell::new(umor::BufferSink::new()));
    let mut interp = Interpreter::with_output(Box::new(sink.clone()));
    interp.load_program(&program);
    interp.run_word("テスト").expect("実行に失敗した");

    assert_eq!(sink.borrow().contents(), "こんにちは\n世界\n");
}
