//! 評価器の出力先を差し替え可能にする抽象化（ADR-0028）。
//!
//! `表示`ワードは、標準出力へ直接書き込むのではなく、この[`OutputSink`]を
//! 経由して書き込む。CLIでは[`StdoutSink`]を、将来のWASM化（Playground）
//! では[`BufferSink`]のような蓄積先を差し込むことを想定する。

use std::cell::RefCell;
use std::rc::Rc;

/// 評価器が出力を書き込む先の抽象。
pub trait OutputSink {
    /// 1行分の文字列を書き込む。改行は実装側が付与する。
    fn write_line(&mut self, s: &str);
}

/// CLI向けのデフォルト実装。標準出力にそのまま書き込む。
pub struct StdoutSink;

impl OutputSink for StdoutSink {
    fn write_line(&mut self, s: &str) {
        println!("{s}");
    }
}

/// WASM向け（将来のPlayground）の実装。文字列をバッファに蓄積する。
#[derive(Default)]
pub struct BufferSink {
    buffer: String,
}

impl BufferSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// これまでに蓄積された出力内容を返す。
    pub fn contents(&self) -> &str {
        &self.buffer
    }
}

impl OutputSink for BufferSink {
    fn write_line(&mut self, s: &str) {
        self.buffer.push_str(s);
        self.buffer.push('\n');
    }
}

// `Rc<RefCell<BufferSink>>`にも`OutputSink`を実装しておく。呼び出し側
// （テスト・将来のWASM側）は`Interpreter::with_output`に渡す前に
// クローンを保持しておけば、実行後もバッファ内容を直接参照できる。
impl OutputSink for Rc<RefCell<BufferSink>> {
    fn write_line(&mut self, s: &str) {
        self.borrow_mut().write_line(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_sink_accumulates_lines() {
        let mut sink = BufferSink::new();
        sink.write_line("こんにちは");
        sink.write_line("世界");
        assert_eq!(sink.contents(), "こんにちは\n世界\n");
    }
}
