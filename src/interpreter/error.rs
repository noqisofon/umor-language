//! 評価器（インタプリタ）のエラー型。

use std::fmt;

/// 実行時エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// 辞書にも変数にも一致しなかったワード呼び出し。
    UndefinedWord(String),
    /// スタックが空の状態でpopしようとした。
    StackUnderflow,
    /// スタック上の値の型が期待と異なる。
    TypeMismatch { expected: String, found: String },
    /// 0での除算。
    DivisionByZero,
    /// 配列の添字が範囲外。
    IndexOutOfBounds { index: i64, length: usize },
    /// 未初期化の変数を読み取ろうとした。
    UninitializedVariable(String),
    /// 実行の正常な打ち切りを表す制御シグナル（ADR-0001）。
    ///
    /// `終了`・`さよなら`のようなREPL脱出ワードが返す。通常のエラーとは異なり、
    /// ユーザーへ提示すべき異常事態ではない。[`crate::interpreter::Interpreter::process_top_level_item`]が
    /// この変種を捕捉し、[`crate::interpreter::ExecutionOutcome::Exit`]へ変換して
    /// 呼び出し元（REPLループ・ファイル実行ループ）へ伝える。既存のネイティブワード
    /// 実装（`Result<(), RuntimeError>`を返すもの）への影響なしに、`?`によって
    /// 通常のエラーと同じ経路でここまで伝播してくる。
    Exit,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::UndefinedWord(name) => write!(f, "未定義のワードです: 「{name}」"),
            RuntimeError::StackUnderflow => write!(f, "スタックの要素が不足しています"),
            RuntimeError::TypeMismatch { expected, found } => write!(
                f,
                "型が一致しません（期待: 「{expected}」, 実際: 「{found}」）"
            ),
            RuntimeError::DivisionByZero => write!(f, "0で除算しようとしました"),
            RuntimeError::IndexOutOfBounds { index, length } => {
                write!(f, "添字が範囲外です（添字: {index}, 配列の長さ: {length}）")
            }
            RuntimeError::UninitializedVariable(name) => {
                write!(f, "変数「{name}」はまだ値が代入されていません")
            }
            RuntimeError::Exit => write!(f, "実行を終了します"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// [`RuntimeError`]に、発生時点の実行コンテキスト（呼び出し中のワード名の列・
/// スタックの状態）を添えたもの。エラーメッセージの表示用。
#[derive(Debug, Clone)]
pub struct RuntimeErrorReport {
    pub error: RuntimeError,
    /// 外側から内側へ向かう、エラー発生時点で実行中だったワード名の列。
    pub word_trace: Vec<String>,
    /// エラー発生時点のスタックの内容（表示用に文字列化済み）。
    pub stack_snapshot: Vec<String>,
}

impl fmt::Display for RuntimeErrorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "実行時エラー: {}", self.error)?;
        if !self.word_trace.is_empty() {
            write!(f, "\n  実行中のワード: {}", self.word_trace.join(" → "))?;
        }
        write!(f, "\n  スタック: [{}]", self.stack_snapshot.join(", "))
    }
}
