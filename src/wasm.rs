//! ブラウザ（Playground）向けのWASMエントリポイント（ADR-0027）。
//!
//! `output.rs`のコメントで示唆されている通り、`BufferSink`を
//! `Rc<RefCell<_>>`で包んで`Interpreter::with_output`に渡すことで、
//! 実行後もクローンした参照から蓄積済みの出力内容を直接読み出せる。

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use wasm_bindgen::prelude::*;

use crate::interpreter::BufferSink;
use crate::{run_source, Interpreter};

/// Umorのソースコードを1回だけ実行し、出力またはエラーメッセージを返す。
///
/// 戻り値は成功・失敗を問わず単一の`String`にまとめる（初期版の簡略化）。
/// 成功時は`表示`等で出力された内容、失敗時は`エラー: `または
/// `内部エラー: `で始まるメッセージを返す。関数名・シグネチャはJS側の
/// 呼び出しコードが依存するため変更しないこと。
#[wasm_bindgen]
pub fn run_umor(source: &str) -> String {
    console_error_panic_hook::set_once();

    let sink = Rc::new(RefCell::new(BufferSink::new()));
    let mut interp = Interpreter::with_output(Box::new(sink.clone()));

    let result = panic::catch_unwind(AssertUnwindSafe(|| run_source(&mut interp, source)));

    match result {
        Ok(Ok(_outcome)) => sink.borrow().contents().to_string(),
        Ok(Err(e)) => format!("エラー: {e}"),
        Err(_) => "内部エラー: 実行中にパニックが発生しました".to_string(),
    }
}
