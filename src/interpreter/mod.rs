//! Umorの評価器（ツリーウォークインタプリタ）。
//!
//! パーサーが出力する[`Program`]/[`Definition`]/[`Expr`]を、再帰的に
//! たどりながらデータスタックを操作して実行する。バイトコードVMへの
//! 発展は将来の課題とし、今回のスコープには含めない。

mod error;
mod output;
mod value;

pub use error::{RuntimeError, RuntimeErrorReport};
pub use output::{BufferSink, OutputSink, StdoutSink};
pub use value::Value;

/// [`Interpreter::process_top_level_item`]の実行結果。
///
/// 通常のエラーとは別に、実行の正常な打ち切り（ADR-0001の`終了`・`さよなら`）を
/// 表現するための型。`RuntimeError::Exit`が`process_top_level_item`の内部で
/// この`Exit`へ変換される。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// 通常通り処理を継続する。
    Continue,
    /// 実行を打ち切り、呼び出し元（REPLループ・ファイル実行ループ）へ
    /// 正常終了の意思を伝える。
    Exit,
}

use crate::parser::{Definition, Expr, Program, TopLevelItem};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// 基本ワードのネイティブ実装。クロージャを保持できるよう`Rc<dyn Fn>`で持つ
/// （テストダブルなど、状態を捕捉したい実装を差し込めるようにするため）。
pub type NativeFn = Rc<dyn Fn(&mut Interpreter) -> Result<(), RuntimeError>>;

/// 辞書に登録されるワードの実体。
#[derive(Clone)]
enum WordDefinition {
    /// 基本ワード（Rustネイティブ実装）。
    Native(NativeFn),
    /// ユーザー定義ワード。`locals`はこのワードが属するトップレベル定義ツリー
    /// 全体で共有される、局所処理単語の名前引きテーブル（同じツリー内の
    /// 兄弟局所処理単語同士も、互いをワードとして呼び出せる）。
    UserDefined {
        def: Rc<Definition>,
        locals: Rc<HashMap<String, Rc<Definition>>>,
    },
}

/// 変数の実体。`Rc<RefCell<..>>`により、親の変数を子（局所処理単語）が
/// 共有・書き換えできる（エイリアシング）。`None`は未初期化を表す。
pub(crate) type VarSlot = Rc<RefCell<Option<Value>>>;

/// 可変値・定数値の実体（ADR-0020）。`変数`（`VarSlot`）とは異なり
/// アドレスを持たず、名前を書いた時点で`cell`の中身がそのまま
/// スタックに積まれる。`Rc<RefCell<..>>`により、`変数`と同様に
/// 親の可変値を子（局所処理単語）が共有・書き換えできる。
#[derive(Clone)]
struct ValueSlot {
    cell: Rc<RefCell<Value>>,
    is_constant: bool,
}

/// 実行中のワード呼び出し1回分のコンテキスト（ADR-0008・ADR-0009向け）。
///
/// `Interpreter::call`が呼ばれるたびに1つ積まれ、戻るときに外される。
#[derive(Clone)]
struct ActiveCall {
    /// このフレームで実行中のワード名（`再帰`の呼び出し先を組み立てる際に使う）。
    name: Rc<str>,
    /// このフレームで実行中の定義本体。`再帰`はこれをそのまま再度呼び出す。
    def: Rc<Definition>,
    /// このフレームが属するトップレベル定義ツリーの局所処理単語テーブル。
    locals: Option<Rc<HashMap<String, Rc<Definition>>>>,
    /// ADR-0008: このフレームが辞書の世代`N`のエントリであれば`Some((name, N))`。
    /// 局所処理単語の呼び出し（辞書引きを経由しない）では`None`。
    /// 本体中で自分と同じ名前（`self_ref.0`）を呼んだ場合、辞書引きは
    /// 世代`N`未満（＝このエントリが追加される前の辞書状態）に限定される。
    self_ref: Option<(Rc<str>, usize)>,
}

/// Umorの評価器本体。
pub struct Interpreter {
    /// データスタック。変数名の`WordCall`は、値を即座に読み取るのではなく
    /// `Value::VarRef`として積む（Phase 1では自動解決しない。[`Interpreter::pop_value`]参照）。
    stack: Vec<Value>,
    /// ADR-0008: 辞書は追記専用。同名ワードの再定義は末尾への追加（新しい世代）
    /// として扱い、削除・上書きは行わない。名前解決は既定では最新の世代
    /// （末尾）から行うが、自己言及的な参照（[`ActiveCall::self_ref`]参照）は
    /// それより古い世代に限定される。
    dictionary: HashMap<String, Vec<WordDefinition>>,
    /// 外側（祖先）から内側（現在実行中）へ向かう、変数フレームのスタック。
    /// フレーム`i`は、祖先チェーン上のある`Definition`が宣言した変数の集合。
    scope_chain: Vec<HashMap<String, VarSlot>>,
    /// 外側（祖先）から内側（現在実行中）へ向かう、可変値・定数値フレームの
    /// スタック（ADR-0020）。`変数`用の`scope_chain`とは独立した別テーブル。
    /// `変数`と対称な親子スコープ構造を持ち、`call`メソッドで`scope_chain`と
    /// 対でpush/popする。`変数`とは異なり事前一括収集はせず、`可変値`宣言を
    /// 実行した瞬間にその時点でアクティブな（末尾の）フレームへ挿入する。
    value_scope_chain: Vec<HashMap<String, ValueSlot>>,
    /// 外側（祖先）から内側（現在実行中）へ向かう、実行中のワード呼び出しの
    /// スタック。局所処理単語の名前引きテーブル（[`ActiveCall::locals`]）・
    /// ADR-0008の世代境界（[`ActiveCall::self_ref`]）・ADR-0009の`再帰`が
    /// 参照する「現在コンパイル中の定義」（[`ActiveCall::def`]）を兼ねる。
    call_stack: Vec<ActiveCall>,
    /// ADR-0010: 回数指定ループ（CountedLoop）の現在の反復回数（0オリジン）の
    /// スタック。ネストしたループでは内側が末尾に積まれ、暗黙変数「回数」の
    /// 参照時に最内側のインデックスを返す。
    counted_loop_stack: Vec<i64>,
    /// エラー時のコンテキスト表示用に、実行中のワード名を外側から積んでいく。
    call_trace: Vec<String>,
    /// ADR-0028: `表示`ワード等の出力系ワードが書き込む先。CLIでは標準出力、
    /// 将来のWASM化ではバッファに差し替えられるよう抽象化してある。
    output: Box<dyn OutputSink>,
    /// ADR-0022: `必要`が読み込み済みと判定するための、正規化済みパスの集合。
    /// `含める`はこれを一切参照しない（無条件に読み込むため）。
    loaded_paths: std::collections::HashSet<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::with_output(Box::new(StdoutSink))
    }

    /// 出力先を明示的に指定してインタプリタを構築する（ADR-0028）。
    pub fn with_output(output: Box<dyn OutputSink>) -> Self {
        let mut interp = Interpreter {
            stack: Vec::new(),
            dictionary: HashMap::new(),
            scope_chain: vec![HashMap::new()], // ADR-0026: トップレベル用の土台フレーム
            value_scope_chain: vec![HashMap::new()], // ADR-0020: 同上（可変値・定数値用）
            call_stack: Vec::new(),
            counted_loop_stack: Vec::new(),
            call_trace: Vec::new(),
            output,
            loaded_paths: std::collections::HashSet::new(),
        };
        register_builtins(&mut interp);
        interp
    }

    /// 出力先への可変参照を返す。`BufferSink`使用時にテストや将来のWASM側から
    /// 出力内容を取り出す際の足がかりとして用意してある（ADR-0028の宿題）。
    pub fn output_mut(&mut self) -> &mut dyn OutputSink {
        self.output.as_mut()
    }

    /// 基本ワード（またはテスト用のダミーワード）をネイティブ実装として登録する。
    /// ADR-0008により、既に同名のワードが登録されていても上書きはせず、
    /// より新しい世代として追加する（名前解決は既定で最新の世代を選ぶため、
    /// 見かけ上は上書きしたのと同じ効果になる）。
    pub fn register_native(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&mut Interpreter) -> Result<(), RuntimeError> + 'static,
    ) {
        let name_str = name.into();
        let normalized = crate::tokenizer::normalize_word(&name_str);
        self.dictionary
            .entry(normalized)
            .or_default()
            .push(WordDefinition::Native(Rc::new(f)));
    }

    /// `program`に含まれる各トップレベル定義を辞書に登録する（実行はしない）。
    pub fn load_program(&mut self, program: &Program) {
        for def in &program.definitions {
            self.load_definition(def);
        }
    }

    fn load_definition(&mut self, def: &Definition) {
        let locals = def
            .locals
            .iter()
            .map(|local| (local.name.clone(), Rc::new(local.clone())))
            .collect();
        self.dictionary
            .entry(def.name.clone())
            .or_default()
            .push(WordDefinition::UserDefined {
                def: Rc::new(def.clone()),
                locals: Rc::new(locals),
            });
    }

    /// 指定した名前のワードを1つ実行する（テスト・動作確認用のエントリポイント）。
    pub fn run_word(&mut self, name: &str) -> Result<(), RuntimeError> {
        self.dispatch(name).map_err(map_break_outside_loop)
    }

    /// [`TopLevelItem`]を1つ処理する。ワード定義なら辞書へ登録するだけ（実行しない）、
    /// トップレベル式の列なら即座に評価する。ファイル実行・REPL共通コア
    /// （[`crate::run_source`]）から、逐次パースしたトップレベル要素ごとに呼ばれる。
    pub fn process_top_level_item(
        &mut self,
        item: &TopLevelItem,
    ) -> Result<ExecutionOutcome, RuntimeError> {
        match item {
            TopLevelItem::Definition(def) => {
                self.load_definition(def);
                Ok(ExecutionOutcome::Continue)
            }
            TopLevelItem::Expr(exprs) => match self.eval_exprs(exprs) {
                Ok(()) => Ok(ExecutionOutcome::Continue),
                Err(RuntimeError::Exit) => Ok(ExecutionOutcome::Exit),
                Err(e) => Err(map_break_outside_loop(e)),
            },
        }
    }

    /// [`run_word`](Self::run_word)などが返した[`RuntimeError`]に、実行時点の
    /// コンテキスト（ワードの呼び出し列・スタックの状態）を添えて報告用に整形する。
    pub fn report(&self, error: RuntimeError) -> RuntimeErrorReport {
        RuntimeErrorReport {
            error,
            word_trace: self.call_trace.clone(),
            stack_snapshot: self.stack.iter().map(|v| v.to_string()).collect(),
        }
    }

    /// データスタックの一番上を取り出す。`Value::VarRef`（変数参照）はそのまま
    /// 返し、現在値へは解決しない（Phase 1では「読」ワードが明示的に解決を行う）。
    pub fn pop_value(&mut self) -> Result<Value, RuntimeError> {
        self.stack.pop().ok_or(RuntimeError::StackUnderflow)
    }

    /// データスタックへ値を積む。
    pub fn push_value(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// データスタックに残っている要素数。
    pub fn stack_len(&self) -> usize {
        self.stack.len()
    }

    /// スタックの上からn番目（0=トップ）を、消費せずに覗き見る（`摘み`用）。
    fn stack_peek(&self, idx_from_top: usize) -> Result<&Value, RuntimeError> {
        let len = self.stack.len();
        if idx_from_top >= len {
            return Err(RuntimeError::StackUnderflow);
        }
        Ok(&self.stack[len - 1 - idx_from_top])
    }

    /// スタックの上からidx_from_top番目の値をその場から抜き取り、
    /// トップに積み直す（`転`/roll相当）。
    fn stack_remove_from_top(&mut self, idx_from_top: usize) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if idx_from_top >= len {
            return Err(RuntimeError::StackUnderflow);
        }
        let value = self.stack.remove(len - 1 - idx_from_top);
        self.stack.push(value);
        Ok(())
    }

    /// スタック全体を下から上の順に表示用文字列へ整形する（`スタック表示`用）。
    fn stack_snapshot_display(&self) -> String {
        self.stack
            .iter()
            .map(|v| format!("{v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// データスタックの一番上を、代入先（変数参照）として取り出す。
    /// 変数参照でなければ`TypeMismatch`。
    fn pop_var_ref(&mut self) -> Result<VarSlot, RuntimeError> {
        match self.pop_value()? {
            Value::VarRef(slot, _) => Ok(slot),
            other => Err(RuntimeError::TypeMismatch {
                expected: "変数".to_string(),
                found: other.type_name().to_string(),
            }),
        }
    }

    fn lookup_variable(&self, name: &str) -> Option<VarSlot> {
        self.scope_chain
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).cloned())
    }

    /// `lookup_variable`と対称な、可変値・定数値の探索（内側から外側）。
    fn lookup_value_slot(&self, name: &str) -> Option<ValueSlot> {
        self.value_scope_chain
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).cloned())
    }

    /// `Expr`列を先頭から順に評価する。
    fn eval_exprs(&mut self, exprs: &[Expr]) -> Result<(), RuntimeError> {
        for expr in exprs {
            self.eval_expr(expr)?;
        }
        Ok(())
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<(), RuntimeError> {
        match expr {
            Expr::WordCall(name) => self.dispatch(name),
            Expr::VariableDecl(name) => {
                // ADR-0026: 土台フレーム（scope_chainの先頭）に新しい空スロットを
                // 追加する。同名の再宣言は既存スロットを新しいものに置き換える
                // （単純な上書き。辞書のような世代管理はしない）。
                self.scope_chain[0].insert(name.clone(), Rc::new(RefCell::new(None)));
                Ok(())
            }
            Expr::AliasDecl {
                new_name,
                existing_name,
            } => {
                // ADR-0030: `existing_name`と同じ実体を指す新しい辞書エントリを
                // 追加する（既存エントリの書き換えではないため、ADR-0008の
                // append-only原則と衝突しない）。
                let existing = existing_name.clone();
                self.register_native(new_name.clone(), move |interp| interp.dispatch(&existing));
                Ok(())
            }
            Expr::ValueDecl {
                name,
                is_constant,
                init_expr,
            } => {
                // ADR-0020: 宣言した瞬間（実行順の到達時点）に`init_expr`を
                // 評価し、その結果をそのまま初期値として束縛する（`変数`の
                // ような事前一括収集ではない）。同名の再宣言は、現在アクティブな
                // フレームへ新しいスロットを上書き挿入する（新しいスロットに
                // 置き換わる。既存の参照者へは影響しない）。
                self.eval_exprs(init_expr)?;
                let value = require_concrete(self.pop_value()?)?;
                let slot = ValueSlot {
                    cell: Rc::new(RefCell::new(value)),
                    is_constant: *is_constant,
                };
                self.value_scope_chain
                    .last_mut()
                    .expect("value_scope_chainは常に非空")
                    .insert(name.clone(), slot);
                Ok(())
            }
            Expr::Assign { name, value_expr } => {
                // ADR-0031: `可変値`への再設定。`value_expr`（現状のパーサー
                // 実装では常に空）を評価してからスタックトップを新しい値として
                // 取り出す。対象名はパーサーが構造キーワードとして直接埋め込んで
                // いるため（`WordCall`としての評価は起きない）、ここで改めて
                // 名前解決する。
                self.eval_exprs(value_expr)?;
                let value = require_concrete(self.pop_value()?)?;
                let slot = self
                    .lookup_value_slot(name)
                    .ok_or_else(|| RuntimeError::UndefinedWord(name.clone()))?;
                if slot.is_constant {
                    return Err(RuntimeError::AssignToConstant(name.clone()));
                }
                *slot.cell.borrow_mut() = value;
                Ok(())
            }
            Expr::NumberLiteral(n) => {
                self.push_value(Value::Number(*n));
                Ok(())
            }
            Expr::StringLiteral(s) => {
                self.push_value(Value::String(Rc::from(s.as_str())));
                Ok(())
            }
            Expr::SelfRecurse => {
                // ADR-0009: パーサーが、どの定義本体にも属さない文脈での
                // `再帰`を構文エラーとして弾いているため、実行時にここへ
                // 到達する時点で必ず`call_stack`は非空。
                let frame = self
                    .call_stack
                    .last()
                    .cloned()
                    .expect("「再帰」は定義本体の外では実行されないはず（パーサーが保証する）");
                self.call(&frame.name, frame.def, frame.locals, frame.self_ref)
            }
            Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            } => {
                self.eval_exprs(cond)?;
                match self.pop_value()? {
                    Value::Bool(true) => self.eval_exprs(then_branch),
                    Value::Bool(false) => match else_branch {
                        Some(branch) => self.eval_exprs(branch),
                        None => Ok(()),
                    },
                    other => Err(RuntimeError::TypeMismatch {
                        expected: "真偽値".to_string(),
                        found: other.type_name().to_string(),
                    }),
                }
            }
            Expr::Break => Err(RuntimeError::Break),
            Expr::InfiniteLoop { body } => {
                loop {
                    match self.eval_exprs(body) {
                        Ok(()) => {}
                        Err(RuntimeError::Break) => break,
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
            Expr::CountedLoop { body } => {
                let n = pop_number(self)?;
                for i in 0..n {
                    self.counted_loop_stack.push(i);
                    let res = self.eval_exprs(body);
                    self.counted_loop_stack.pop();
                    match res {
                        Ok(()) => {}
                        Err(RuntimeError::Break) => break,
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
        }
    }

    /// `WordCall(name)`の実行本体。文字リテラルの脱糖衣、変数の読み取り、
    /// 局所処理単語・ユーザー定義ワード・基本ワードの呼び出しを順に試す。
    fn dispatch(&mut self, name: &str) -> Result<(), RuntimeError> {
        if let Some(content) = strip_char_literal(name) {
            self.push_value(Value::String(Rc::from(content)));
            return Ok(());
        }

        if name == "回数" {
            if let Some(&i) = self.counted_loop_stack.last() {
                self.push_value(Value::Number(i));
                return Ok(());
            }
        }

        if let Some(slot) = self.lookup_variable(name) {
            self.push_value(Value::VarRef(slot, Rc::from(name)));
            return Ok(());
        }

        // ADR-0020: `可変値`・`定数値`はアドレスを持たず、名前を書いた時点で
        // 値そのものが直接積まれる（`変数`のような`VarRef`は経由しない）。
        if let Some(slot) = self.lookup_value_slot(name) {
            self.push_value(slot.cell.borrow().clone());
            return Ok(());
        }

        let current_locals = self
            .call_stack
            .last()
            .and_then(|frame| frame.locals.clone());
        if let Some(local_def) = current_locals
            .as_ref()
            .and_then(|locals| locals.get(name).cloned())
        {
            return self.call(name, local_def, current_locals, None);
        }

        // ADR-0008: 名前解決は既定では最新の世代（末尾）から行うが、現在
        // 実行中のワード自身と同じ名前を呼んだ場合（自己言及的な再定義
        // イディオム）は、このワードが辞書に追加される前の世代までに
        // 限定する（＝自分自身の世代は候補から除外する）。
        let self_ref = self
            .call_stack
            .last()
            .and_then(|frame| frame.self_ref.clone());
        let cutoff = match &self_ref {
            Some((self_name, generation)) if self_name.as_ref() == name => *generation,
            _ => self.dictionary.get(name).map(Vec::len).unwrap_or(0),
        };
        if cutoff == 0 {
            return Err(RuntimeError::UndefinedWord(name.to_string()));
        }
        let generation = cutoff - 1;
        match self
            .dictionary
            .get(name)
            .and_then(|gens| gens.get(generation))
            .cloned()
        {
            Some(WordDefinition::Native(f)) => {
                self.call_trace.push(name.to_string());
                let result = f(self);
                self.call_trace.pop();
                result
            }
            Some(WordDefinition::UserDefined { def, locals }) => {
                self.call(name, def, Some(locals), Some((Rc::from(name), generation)))
            }
            None => Err(RuntimeError::UndefinedWord(name.to_string())),
        }
    }

    /// `def`を、`locals`を局所処理単語テーブルとして実行する。`def`自身の
    /// 変数用フレームを積み、本体を評価してからフレームを外す。`self_ref`は
    /// ADR-0008の世代境界（[`ActiveCall::self_ref`]参照）。
    fn call(
        &mut self,
        name: &str,
        def: Rc<Definition>,
        locals: Option<Rc<HashMap<String, Rc<Definition>>>>,
        self_ref: Option<(Rc<str>, usize)>,
    ) -> Result<(), RuntimeError> {
        let frame = def
            .variables
            .iter()
            .map(|v| (v.clone(), Rc::new(RefCell::new(None))))
            .collect();
        self.scope_chain.push(frame);
        // ADR-0020: `可変値`は事前一括収集をしないため、空のフレームを積む
        // （宣言を実行した瞬間に`Expr::ValueDecl`がここへ挿入する）。
        self.value_scope_chain.push(HashMap::new());
        self.call_trace.push(name.to_string());
        self.call_stack.push(ActiveCall {
            name: Rc::from(name),
            def: def.clone(),
            locals,
            self_ref,
        });

        let result = self.eval_exprs(&def.body);

        self.call_stack.pop();
        self.call_trace.pop();
        self.value_scope_chain.pop();
        self.scope_chain.pop();
        result
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/// `'X'`文字リテラルの脱糖衣形から中身の1文字を取り出す。
fn strip_char_literal(name: &str) -> Option<&str> {
    let inner = name.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))?;
    if inner.chars().count() == 1 {
        Some(inner)
    } else {
        None
    }
}

fn map_break_outside_loop(err: RuntimeError) -> RuntimeError {
    if err == RuntimeError::Break {
        RuntimeError::BreakOutsideLoop
    } else {
        err
    }
}

fn pop_number(interp: &mut Interpreter) -> Result<i64, RuntimeError> {
    match interp.pop_value()? {
        Value::Number(n) => Ok(n),
        other => Err(RuntimeError::TypeMismatch {
            expected: "数".to_string(),
            found: other.type_name().to_string(),
        }),
    }
}

fn pop_bool(interp: &mut Interpreter) -> Result<bool, RuntimeError> {
    match interp.pop_value()? {
        Value::Bool(b) => Ok(b),
        other => Err(RuntimeError::TypeMismatch {
            expected: "真偽値".to_string(),
            found: other.type_name().to_string(),
        }),
    }
}

fn pop_array(interp: &mut Interpreter) -> Result<Rc<RefCell<Vec<Value>>>, RuntimeError> {
    match interp.pop_value()? {
        Value::Array(a) => Ok(a),
        other => Err(RuntimeError::TypeMismatch {
            expected: "配列".to_string(),
            found: other.type_name().to_string(),
        }),
    }
}

/// スタックトップを、モジュールのファイルパスとして取り出す（`含める`／`必要`用）。
fn pop_path_string(interp: &mut Interpreter) -> Result<String, RuntimeError> {
    match interp.pop_value()? {
        Value::String(s) => Ok(s.to_string()),
        other => Err(RuntimeError::TypeMismatch {
            expected: "文字列".to_string(),
            found: other.type_name().to_string(),
        }),
    }
}

/// ADR-0022の宿題「パス正規化」への最小の回答: 先頭の`./`のみを1回剥がす。
/// `../`・絶対パス・区切り文字の差異には対応しない（意図的なスコープ外）。
fn normalize_module_path(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

/// `path`の内容を読み込み、現在のインタプリタ状態に対して評価する
/// （`含める`／`必要`共通処理）。ファイルI/OはCLI専用機能と割り切っており、
/// WASM32ターゲットではコンパイルは通るが、実行時に`fs::read_to_string`が
/// 失敗して`ModuleReadError`になるだけでよい（ADR-0022）。
fn include_file(interp: &mut Interpreter, path: &str) -> Result<(), RuntimeError> {
    let src = std::fs::read_to_string(path).map_err(|e| RuntimeError::ModuleReadError {
        path: path.to_string(),
        reason: e.to_string(),
    })?;
    crate::runner::run_source(interp, &src)
        .map(|_outcome| ())
        .map_err(|e| RuntimeError::ModuleError {
            path: path.to_string(),
            message: e.to_string(),
        })
}

/// `value`が`Value::VarRef`（未解決の変数参照）であれば`TypeMismatch`にする。
/// Phase 1では、変数参照を具体的な値として扱えるワードは「読」のみで、
/// それ以外のワード（`表示`を除く）はこれを通して弾く。
fn require_concrete(value: Value) -> Result<Value, RuntimeError> {
    match value {
        Value::VarRef(_, name) => Err(RuntimeError::TypeMismatch {
            expected: "具体的な値".to_string(),
            found: format!("変数参照「{name}」"),
        }),
        other => Ok(other),
    }
}

/// 基本ワードセット、および助詞・添字連結詞など、脱糖衣後のASTを実行するために
/// 実行時にも意味を持つ補助ワードを登録する。
///
/// 「を」「に」などの助詞は、パーサーが読み飛ばさず`WordCall`としてそのまま
/// 残しているため、評価器側で無害な（スタックに影響しない）ワードとして
/// 登録しておく必要がある。
fn register_builtins(interp: &mut Interpreter) {
    interp.register_native("越え", |interp| {
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(a.clone());
        interp.push_value(b);
        interp.push_value(a);
        Ok(())
    });

    interp.register_native("回転", |interp| {
        let c = interp.pop_value()?;
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(b);
        interp.push_value(c);
        interp.push_value(a);
        Ok(())
    });

    interp.register_native("複製", |interp| {
        let top = interp.pop_value()?;
        interp.push_value(top.clone());
        interp.push_value(top);
        Ok(())
    });

    interp.register_native("取替", |interp| {
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(b);
        interp.push_value(a);
        Ok(())
    });

    interp.register_native("捨", |interp| {
        interp.pop_value()?;
        Ok(())
    });

    // ADR-0029: フェーズ1で追加されたスタック操作ワード群。

    interp.register_native("深さ", |interp| {
        let len = interp.stack_len() as i64;
        interp.push_value(Value::Number(len));
        Ok(())
    });

    interp.register_native("二捨", |interp| {
        interp.pop_value()?;
        interp.pop_value()?;
        Ok(())
    });

    interp.register_native("二複製", |interp| {
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(a.clone());
        interp.push_value(b.clone());
        interp.push_value(a);
        interp.push_value(b);
        Ok(())
    });

    interp.register_native("二交換", |interp| {
        let d = interp.pop_value()?;
        let c = interp.pop_value()?;
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(c);
        interp.push_value(d);
        interp.push_value(a);
        interp.push_value(b);
        Ok(())
    });

    interp.register_native("取替捨", |interp| {
        let b = interp.pop_value()?;
        interp.pop_value()?; // a を捨てる
        interp.push_value(b);
        Ok(())
    });

    interp.register_native("取替越", |interp| {
        let b = interp.pop_value()?;
        let a = interp.pop_value()?;
        interp.push_value(b.clone());
        interp.push_value(a);
        interp.push_value(b);
        Ok(())
    });

    interp.register_native("摘み", |interp| {
        let n = pop_number(interp)?;
        if n < 0 {
            return Err(RuntimeError::TypeMismatch {
                expected: "0以上の整数".to_string(),
                found: n.to_string(),
            });
        }
        let idx_from_top = n as usize;
        let value = interp.stack_peek(idx_from_top)?.clone();
        interp.push_value(value);
        Ok(())
    });

    interp.register_native("転", |interp| {
        let n = pop_number(interp)?;
        if n < 0 {
            return Err(RuntimeError::TypeMismatch {
                expected: "0以上の整数".to_string(),
                found: n.to_string(),
            });
        }
        let idx_from_top = n as usize;
        interp.stack_remove_from_top(idx_from_top)?;
        Ok(())
    });

    interp.register_native("スタック表示", |interp| {
        let s = interp.stack_snapshot_display();
        interp.output_mut().write_line(&s);
        Ok(())
    });

    interp.register_native("加", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Number(a + b));
        Ok(())
    });

    interp.register_native("引", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Number(a - b));
        Ok(())
    });

    interp.register_native("掛", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Number(a * b));
        Ok(())
    });

    interp.register_native("割", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        if b == 0 {
            return Err(RuntimeError::DivisionByZero);
        }
        interp.push_value(Value::Number(a / b));
        Ok(())
    });

    interp.register_native("余", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        if b == 0 {
            return Err(RuntimeError::DivisionByZero);
        }
        interp.push_value(Value::Number(a % b));
        Ok(())
    });

    interp.register_native("等しい?", |interp| {
        let b = require_concrete(interp.pop_value()?)?;
        let a = require_concrete(interp.pop_value()?)?;
        interp.push_value(Value::Bool(a == b));
        Ok(())
    });

    interp.register_native("異なる?", |interp| {
        let b = require_concrete(interp.pop_value()?)?;
        let a = require_concrete(interp.pop_value()?)?;
        interp.push_value(Value::Bool(a != b));
        Ok(())
    });

    interp.register_native("違う?", |interp| {
        let b = require_concrete(interp.pop_value()?)?;
        let a = require_concrete(interp.pop_value()?)?;
        interp.push_value(Value::Bool(a != b));
        Ok(())
    });

    interp.register_native("大?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a > b));
        Ok(())
    });

    interp.register_native("超える?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a > b));
        Ok(())
    });

    interp.register_native("小?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a < b));
        Ok(())
    });

    interp.register_native("未満?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a < b));
        Ok(())
    });

    interp.register_native("以上?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a >= b));
        Ok(())
    });

    interp.register_native("以下?", |interp| {
        let b = pop_number(interp)?;
        let a = pop_number(interp)?;
        interp.push_value(Value::Bool(a <= b));
        Ok(())
    });

    interp.register_native("かつ", |interp| {
        let b = pop_bool(interp)?;
        let a = pop_bool(interp)?;
        interp.push_value(Value::Bool(a && b));
        Ok(())
    });

    interp.register_native("または", |interp| {
        let b = pop_bool(interp)?;
        let a = pop_bool(interp)?;
        interp.push_value(Value::Bool(a || b));
        Ok(())
    });

    interp.register_native("番目", |interp| {
        let index = pop_number(interp)?;
        let array = pop_array(interp)?;
        let borrowed = array.borrow();
        let len = borrowed.len();
        if index < 0 || index as usize >= len {
            return Err(RuntimeError::IndexOutOfBounds { index, length: len });
        }
        let value = borrowed[index as usize].clone();
        drop(borrowed);
        interp.push_value(value);
        Ok(())
    });

    // 「入れる」の送り仮名除去後の形（「いれる」ではなく「入れる」が正しい語形）。
    interp.register_native("入", |interp| {
        let slot = interp.pop_var_ref()?;
        let value = require_concrete(interp.pop_value()?)?;
        *slot.borrow_mut() = Some(value);
        Ok(())
    });

    // 変数参照（`Value::VarRef`）を、その時点の現在値へ明示的に解決する。
    // Phase 1では他のワードは変数参照を自動解決しないため、値として使う前に
    // このワードを挟む必要がある（例外は`表示`。文字列表現をそのまま出力する）。
    interp.register_native("読", |interp| {
        let value = interp.pop_value()?;
        match value {
            Value::VarRef(slot, name) => {
                let resolved = slot
                    .borrow()
                    .clone()
                    .ok_or_else(|| RuntimeError::UninitializedVariable(name.to_string()))?;
                interp.push_value(resolved);
            }
            other => interp.push_value(other),
        }
        Ok(())
    });

    interp.register_native("表示", |interp| {
        let value = interp.pop_value()?;
        let s = format!("{value}");
        interp.output.write_line(&s);
        Ok(())
    });

    // 添字アクセス糖衣構文（`（）`）の脱糖衣で挿入される連結詞。単体では何もしない。
    interp.register_native("の", |_interp| Ok(()));

    // 助詞。文法上あちこちに現れるが、実行時には何もしない。
    for particle in [
        "を", "に", "と", "で", "が", "へ", "も", "から", "まで", "や",
    ] {
        interp.register_native(particle, |_interp| Ok(()));
    }

    // 配列を組み立てるための最低限のワード（指示書には明記されていないが、
    // テスト・動作確認のために用意する）。
    interp.register_native("空配列", |interp| {
        interp.push_value(Value::Array(Rc::new(RefCell::new(Vec::new()))));
        Ok(())
    });

    interp.register_native("追加", |interp| {
        let value = require_concrete(interp.pop_value()?)?;
        let array = pop_array(interp)?;
        array.borrow_mut().push(value);
        interp.push_value(Value::Array(array));
        Ok(())
    });

    // REPL・ファイル実行を打ち切るワード（ADR-0001）。専用コマンド層は持たず、
    // 辞書引きより優先させない通常ワードとして提供する。再定義すればその
    // 定義が実行され、このワード本来の終了動作は失われる（意図した挙動）。
    interp.register_native("終了", |_interp| Err(RuntimeError::Exit));
    interp.register_native("さよなら", |_interp| Err(RuntimeError::Exit));

    // ADR-0030: 交換は取替の別名（ADR-0017で判明した重複の解消）。
    // Umor起動時に読み込む「標準ライブラリ相当のコード」として
    // `交換も 取替の 別名。`を実際に評価する仕組み（起動時Umorコード読み込み）は
    // 現状存在しないため、`Expr::AliasDecl`の評価ロジック（`dispatch`委譲）と
    // 同じ形をRust側で直接記述することで代替する。
    interp.register_native("交換", |interp| interp.dispatch("取替"));

    // ADR-0022: モジュール読み込み機構。`含める`は無条件読み込み（Forthの
    // INCLUDE相当）、`必要`は同一パスの重複読み込みを防ぐ（REQUIRE相当）。
    interp.register_native("含める", |interp| {
        let path = pop_path_string(interp)?;
        include_file(interp, &path)
    });

    interp.register_native("必要", |interp| {
        let path = pop_path_string(interp)?;
        let normalized = normalize_module_path(&path);
        if interp.loaded_paths.contains(&normalized) {
            return Ok(());
        }
        include_file(interp, &path)?;
        interp.loaded_paths.insert(normalized);
        Ok(())
    });
}
