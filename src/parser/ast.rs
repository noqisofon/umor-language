//! パーサーが構築する抽象構文木（AST）の定義。

/// プログラム全体。トップレベルのワード定義の列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub definitions: Vec<Definition>,
}

/// 1つのワード定義（トップレベル、または局所処理単語）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    /// 局所処理単語（再帰的にネスト可）。
    pub locals: Vec<Definition>,
    /// このDefinitionと配下のlocalsで共有される変数名。
    pub variables: Vec<String>,
    /// 本体（`本体とは` の中身、または局所定義を持たない場合の中身）。
    pub body: Vec<Expr>,
}

/// 本体を構成する式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// 通常のワード呼び出し。変数の読み取り・代入もここに含む。
    WordCall(String),
    /// 助数詞正規化済みの数値トークン。
    NumberLiteral(i64),
    /// `＜条件＞ ならば ＜then節＞ [そうでなければ ＜else節＞] つぎに`
    IfElse {
        cond: Vec<Expr>,
        then_branch: Vec<Expr>,
        else_branch: Option<Vec<Expr>>,
    },
}
