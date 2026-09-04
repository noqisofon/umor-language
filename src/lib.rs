//! Umor 言語処理系のライブラリクレート。
//!
//! 現時点では字句解析器（tokenizer）のみを提供する。
//! パーサ・単語辞書・スタックマシン・型検査器は後続タスクで実装する。

pub mod tokenizer;

pub use tokenizer::{tokenize, Token, TokenKind};
