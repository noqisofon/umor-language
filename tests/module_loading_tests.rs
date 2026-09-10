//! impl-0032: モジュール読み込み機構（`含める`／`必要`, ADR-0022）の受け入れテスト。

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use umor::{run_source, Interpreter, RuntimeError, UmorError};

/// テストダブル: `表示`を、標準出力の代わりに`log`へ蓄積するよう差し替える。
fn install_logging_display(interp: &mut Interpreter, log: Rc<RefCell<Vec<String>>>) {
    interp.register_native("表示", move |interp| {
        let value = interp.pop_value()?;
        log.borrow_mut().push(value.to_string());
        Ok(())
    });
}

/// テスト用の一時`.umor`ファイル。`Drop`で自動的に削除する。
/// （`tempfile`クレート等の新規依存は追加しない）
struct TempUmorFile {
    path: PathBuf,
}

impl TempUmorFile {
    fn create(dir: PathBuf, content: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = dir.join(format!("umor_module_test_{}_{n}.umor", std::process::id()));
        fs::write(&path, content).expect("一時ファイルの書き込みに失敗した");
        TempUmorFile { path }
    }

    /// `std::env::temp_dir()`配下に、絶対パスで参照する一時ファイルを作る。
    fn in_temp_dir(content: &str) -> Self {
        Self::create(std::env::temp_dir(), content)
    }

    /// カレントディレクトリ配下に一時ファイルを作る（相対パス表記ゆれの検証用）。
    fn in_current_dir(content: &str) -> Self {
        Self::create(
            std::env::current_dir().expect("カレントディレクトリの取得に失敗した"),
            content,
        )
    }

    fn path_str(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    fn file_name_str(&self) -> String {
        self.path
            .file_name()
            .expect("ファイル名が取得できるはず")
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for TempUmorFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn kakumeru_evaluates_the_file_unconditionally_every_time() {
    let file = TempUmorFile::in_temp_dir("「loaded」を　表示する。");
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!(
        "「{path}」を　含める。\n「{path}」を　含める。",
        path = file.path_str()
    );
    run_source(&mut interp, &src).expect("実行に失敗した");

    assert_eq!(
        *log.borrow(),
        vec!["loaded".to_string(), "loaded".to_string()]
    );
}

#[test]
fn hitsuyou_loads_only_once_for_the_same_path() {
    let file = TempUmorFile::in_temp_dir("「loaded」を　表示する。");
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!(
        "「{path}」が　必要。\n「{path}」が　必要。",
        path = file.path_str()
    );
    run_source(&mut interp, &src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["loaded".to_string()]);
}

#[test]
fn hitsuyou_treats_dot_slash_prefix_as_the_same_path() {
    let file = TempUmorFile::in_current_dir("「loaded」を　表示する。");
    let name = file.file_name_str();
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!("「{name}」が　必要。\n「./{name}」が　必要。");
    run_source(&mut interp, &src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["loaded".to_string()]);
}

#[test]
fn kakumeru_on_a_missing_file_is_a_runtime_error() {
    let mut interp = Interpreter::new();
    let missing_path = "/definitely/does/not/exist/umor_missing_module.umor";
    let src = format!("「{missing_path}」を　含める。");

    let err = run_source(&mut interp, &src).expect_err("存在しないファイルはエラーになるはず");
    match err {
        UmorError::Runtime(report) => match report.error {
            RuntimeError::ModuleReadError { path, .. } => assert_eq!(path, missing_path),
            other => panic!("expected ModuleReadError, got {other:?}"),
        },
        other => panic!("expected Runtime error, got {other}"),
    }
}

#[test]
fn kakumeru_propagates_runtime_errors_from_inside_the_loaded_file() {
    let file = TempUmorFile::in_temp_dir("ぞんざいするワード。");
    let mut interp = Interpreter::new();
    let src = format!("「{path}」を　含める。", path = file.path_str());

    let err =
        run_source(&mut interp, &src).expect_err("読み込んだファイル内のエラーが伝播するはず");
    match err {
        UmorError::Runtime(report) => match report.error {
            RuntimeError::ModuleError { path, message } => {
                assert_eq!(path, file.path_str());
                assert!(message.contains("未定義"), "got {message}");
            }
            other => panic!("expected ModuleError, got {other:?}"),
        },
        other => panic!("expected Runtime error, got {other}"),
    }
}

#[test]
fn kakumeru_shares_dictionary_and_variables_with_the_including_side() {
    let file = TempUmorFile::in_temp_dir("挨拶 とは\n    「こんにちは」を　表示する\nこと。");
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!("「{path}」を　含める。\n\n挨拶。", path = file.path_str());
    run_source(&mut interp, &src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["こんにちは".to_string()]);
}

#[test]
fn hitsuyou_still_shares_state_when_it_actually_loads() {
    let file = TempUmorFile::in_temp_dir("挨拶 とは\n    「こんにちは」を　表示する\nこと。");
    let mut interp = Interpreter::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    install_logging_display(&mut interp, log.clone());

    let src = format!("「{path}」が　必要。\n\n挨拶。", path = file.path_str());
    run_source(&mut interp, &src).expect("実行に失敗した");

    assert_eq!(*log.borrow(), vec!["こんにちは".to_string()]);
}
