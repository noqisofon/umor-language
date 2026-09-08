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

/// トップレベルの要素。ワード定義（辞書に登録するだけで実行しない）と、
/// その場で即座に実行される式の列（`。`までの1単位）のいずれか。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopLevelItem {
    Definition(Definition),
    Expr(Vec<Expr>),
}

/// 本体を構成する式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// 通常のワード呼び出し。変数の読み取り・代入もここに含む。
    WordCall(String),
    /// トップレベルの変数宣言（`Xは 変数`）。ADR-0026。
    VariableDecl(String),
    /// 助数詞正規化済みの数値トークン。
    NumberLiteral(i64),
    /// 文字列リテラル（鍵括弧「...」や二重引用符 "..."）。
    StringLiteral(String),
    /// `＜条件＞ ならば ＜then節＞ [そうでなければ ＜else節＞] つぎに`
    IfElse {
        cond: Vec<Expr>,
        then_branch: Vec<Expr>,
        else_branch: Option<Vec<Expr>>,
    },
    /// `再帰`構文キーワード（ADR-0009）。辞書引きを経由しない、現在
    /// コンパイル中の定義（局所処理単語の内部であれば、その局所処理単語
    /// 自身）への自己参照。
    SelfRecurse,
    /// (A) 無限ループ `ここから ... 繰り返し`（ADR-0010）。
    InfiniteLoop {
        body: Vec<Expr>,
    },
    /// (B) 回数指定ループ `〈回数〉 回数指定し ... 繰り返す`（ADR-0010）。
    CountedLoop {
        body: Vec<Expr>,
    },
    /// ループ離脱 `打ち切り`（ADR-0010）。
    Break,
}
