//! issue #16 / ADR-0010: ループ構文（無限ループ、回数指定ループ、打ち切り、回数暗黙変数）のテスト。

use std::cell::RefCell;
use std::rc::Rc;
use umor::{run_source, Interpreter, Value};

fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}

#[test]
fn test5_1_fibonacci_infinite_loop_with_break() {
    // 5.1 フィボナッチ数（無限ループ版）
    let src = "
フィボナッチ数 とは
    0 1
    ここから
        越えて 50 大きい？ ならば
            打ち切り
        つぎに
        越えて 加え 交換
    繰り返し
こと。
";
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log);

    run_source(&mut interp, src).expect("フィボナッチ数の定義に失敗した");
    run_source(&mut interp, "フィボナッチ数。").expect("フィボナッチ数の実行に失敗した");

    // スタックトップの要素を確認（50以下で最大のフィボナッチ数は 34）
    let val = interp.pop_value().expect("スタックに結果が残っているはず");
    assert_eq!(val, Value::Number(34));
}

#[test]
fn test5_2_counted_loop_basic() {
    // 5.2 回数指定ループの基本動作
    let src = "
5 回数指定し
    回数 表示する
繰り返す。
";
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, src).expect("回数指定ループの実行に失敗した");

    assert_eq!(
        *log.borrow(),
        vec!["0".to_string(), "1".to_string(), "2".to_string(), "3".to_string(), "4".to_string()]
    );
}

#[test]
fn test5_3_infinite_loop_break() {
    // 5.3 打ち切りの動作
    let src = "
カウント とは
    X は 変数
    0 X 入
    ここから
        X 読 表示する
        X 読 3 等しい？ ならば
            打ち切り
        つぎに
        X 読 1 加え X 入
    繰り返し
こと。

カウント。
";
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, src).expect("無限ループと打ち切りの実行に失敗した");

    assert_eq!(
        *log.borrow(),
        vec!["0".to_string(), "1".to_string(), "2".to_string(), "3".to_string()]
    );
}

#[test]
fn test5_4_nested_counted_loop_shadowing() {
    // 5.4 ネストしたCountedLoopでの「回数」シャドーイング
    let src = "
3 回数指定し
    2 回数指定し
        回数 表示する
    繰り返す
繰り返す。
";
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    run_source(&mut interp, src).expect("ネストした回数指定ループの実行に失敗した");

    // 外側3回 × 内側2回（0, 1）で計6回の出力がすべて "0", "1" のペアであること
    assert_eq!(
        *log.borrow(),
        vec![
            "0".to_string(), "1".to_string(),
            "0".to_string(), "1".to_string(),
            "0".to_string(), "1".to_string(),
        ]
    );
}

#[test]
fn break_outside_loop_is_runtime_error() {
    let mut interp = Interpreter::new();
    let err = run_source(&mut interp, "打ち切り。").expect_err("ループ外の打ち切りはエラーになるはず");
    match err {
        umor::UmorError::Runtime(report) => {
            assert_eq!(report.error, umor::RuntimeError::BreakOutsideLoop);
        }
        other => panic!("expected Runtime error, got {other}"),
    }
}
