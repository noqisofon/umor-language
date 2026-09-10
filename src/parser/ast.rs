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
    /// トップレベルのエイリアス宣言（`〈新語〉も 〈既存語〉の 別名`）。ADR-0030。
    AliasDecl {
        new_name: String,
        existing_name: String,
    },
    /// 可変値・定数値の宣言（`Xは 可変値で 〈式〉が 初期値` /
    /// `Xは 定数値で 〈式〉が 初期値`）。ADR-0020。`変数`とは異なり、
    /// 宣言された瞬間（実行順の到達時点）に`init_expr`が評価され、その
    /// 結果がそのまま初期値として束縛される。
    ValueDecl {
        name: String,
        is_constant: bool,
        init_expr: Vec<Expr>,
    },
    /// 可変値への再設定（`〈値の式〉 〈対象名〉 代入`）。ADR-0031のうち
    /// `可変値`への`代入`部分のみ（予約・無名ワードは未実装）。`name`の
    /// 直前に書かれた値の式は、通常の`WordCall`列としてこのノードより
    /// 前に本体へそのまま残る（実行順にスタックへ積まれる）ため、
    /// `value_expr`は現状のパーサー実装では常に空になる。
    Assign { name: String, value_expr: Vec<Expr> },
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
