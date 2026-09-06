//! 評価器が扱う実行時値（`Value`）の定義。

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Umorの実行時値。
///
/// - `Number`・`Bool`はコピーで扱える。
/// - `String`は`Rc<str>`により、複製時も実体は共有される（イミュータブル）。
/// - `Array`は`Rc<RefCell<Vec<Value>>>`により、複数の場所から同じ実体を
///   参照でき、要素の書き換えは共有された実体に反映される（エイリアシング）。
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(i64),
    Bool(bool),
    String(Rc<str>),
    Array(Rc<RefCell<Vec<Value>>>),
}

impl Value {
    /// エラーメッセージ用の型名。
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "数",
            Value::Bool(_) => "真偽値",
            Value::String(_) => "文字列",
            Value::Array(_) => "配列",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{}", if *b { "真" } else { "偽" }),
            Value::String(s) => write!(f, "{s}"),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}
