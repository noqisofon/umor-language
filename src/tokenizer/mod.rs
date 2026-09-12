//! Umorの字句解析器（Tokenizer）。
//!
//! ソースコードを「分かち書き」された単語や各種リテラルのトークン列へ変換する。
//! Mindの構文思想（分かち書き・送り仮名の無視）を踏襲しつつ、実装は
//! シンプルさを優先する。

mod error;
mod kana_table;
mod normalize;
mod okurigana;

pub use error::LexError;
pub use normalize::normalize_width_and_case;
pub use okurigana::{is_hiragana, normalize_okurigana};

/// トークンの種別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// 通常の単語（正規化済み文字列）。
    Word(String),
    /// 文字列リテラルの中身（正規化は適用しない）。
    StringLiteral(String),
    /// 文字リテラル（正規化は適用しない）。
    CharLiteral(char),
    /// 数値リテラル（文字列のまま。パースは後続フェーズに委ねる）。
    NumberLiteral(String),
    /// 添字アクセス糖衣構文の開き括弧（`（`/`(`）。
    ///
    /// 直前のトークンに空白なしで隣接する丸括弧のみがこれになる。
    /// 空白を挟んだ丸括弧はコメントとして読み飛ばされ、トークンにならない。
    OpenParen,
    /// 添字アクセス糖衣構文の閉じ括弧（`）`/`)`）。
    CloseParen,
}

/// 1つのトークン。位置情報とソース上の生表記を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// 正規化前のソースコード上の表記。
    pub raw: String,
    /// トークン開始位置の行番号（1始まり）。
    pub line: usize,
}

/// 区切り文字かどうかを判定する。
///
/// 半角/全角スペース、タブ、半角/全角カンマ、半角/全角読点、
/// および改行（トークンを区切ると同時に行番号管理にも使う）を区切り文字とする。
fn is_delimiter(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r'
            | '\u{3000}' // 全角スペース
            | ','
            | '\u{FF0C}' // 全角カンマ（，）
            | '\u{FF64}' // 半角読点（､）
            | '\u{3001}' // 全角読点（、）
    )
}

/// 数値リテラルを構成しうる文字かどうかを判定する。
///
/// 半角/全角数字、マイナス符号、小数点、指数表記の E/e（半角/全角）を対象とする。
fn is_number_char(c: char) -> bool {
    matches!(
        c,
        '0'..='9'
            | '\u{FF10}'..='\u{FF19}' // 全角数字
            | '-' | '\u{FF0D}' // 半角/全角マイナス
            | '.' | '\u{FF0E}' // 半角/全角小数点
            | 'E' | 'e' | '\u{FF25}' | '\u{FF45}' // 半角/全角 E/e
    )
}

fn is_digit_char(c: char) -> bool {
    matches!(c, '0'..='9' | '\u{FF10}'..='\u{FF19}')
}

/// `chars` の先頭から連続する数値構成文字の長さを返す。
fn number_prefix_len(chars: &[char]) -> usize {
    let mut len = 0;
    while len < chars.len() && is_number_char(chars[len]) {
        len += 1;
    }
    len
}

/// `chars[..from]` から数字（記号を除く）が1つでも含まれるかを判定する。
fn contains_digit(chars: &[char]) -> bool {
    chars.iter().any(|&c| is_digit_char(c))
}

/// 助詞のリスト（最長一致のために複数文字の助詞を先に配置）。ADR-0024。
const PARTICLES: &[&str] = &[
    "から", "まで", "より", // 2文字
    "は", "が", "を", "に", "へ", "と", "で", "も", "や", // 1文字
];

/// 助詞と同じ文字列で終わるが、助詞切り出しを行ってはならない予約キーワード・保護語。
const PROTECTED_KEYWORDS: &[&str] = &[
    "とは",
    "こと",
    "ここから",
    "つぎに",
    "ならば",
    "そうでなければ",
    "さもなければ",
    "さよなら",
];

/// 単語が純ひらがな語であり、既知の助詞で終わっているか判定し、もし終わっていれば `(語幹, 助詞)` を返す。
///
/// 送り仮名正規化（漢字語幹の抽出）の対象外となる純ひらがな語（例: `ほにに`）に対して
/// 2段目のパイプラインとして助詞を切り出す（ADR-0024）。
/// 単語全体が保護キーワードである場合、終端語「こと」「とは」で終わる場合、
/// または単語全体が助詞そのものである場合は `None` を返す。
fn split_trailing_particle(raw: &str) -> Option<(&str, &str)> {
    if PROTECTED_KEYWORDS.contains(&raw) {
        return None;
    }
    // 全文字がひらがな（＋長音記号）の場合のみ助詞切り出しの対象とする
    if !raw.chars().all(|c| is_hiragana(c) || c == 'ー') {
        return None;
    }
    // 構造キーワード「こと」「とは」で終わる語（例: 「すること」）は助詞切り出しを行わない
    if raw != "こと" && raw.ends_with("こと") {
        return None;
    }
    if raw != "とは" && raw.ends_with("とは") {
        return None;
    }
    for &p in PARTICLES {
        if raw.ends_with(p) && raw.len() > p.len() {
            let stem = &raw[..raw.len() - p.len()];
            return Some((stem, p));
        }
    }
    None
}

/// 生トークン（区切り文字を含まない1塊の文字列）を分類し、トークンとして積む。
///
/// 先頭が数値パターンで構成される場合、数値に空白なしで隣接する残り部分
/// （助数詞等）はADR-0002の規定どおり数値トークンへ完全に畳み込まれ、
/// 独立したトークン・ワードとしては一切現れない
/// （例: 「５６０円」→ NumberLiteral("５６０") のみ。「円」は消える）。
fn classify_and_push(raw: &str, line: usize, tokens: &mut Vec<Token>) {
    let chars: Vec<char> = raw.chars().collect();
    let num_len = number_prefix_len(&chars);

    if num_len > 0 && contains_digit(&chars[..num_len]) {
        let num_part: String = chars[..num_len].iter().collect();
        tokens.push(Token {
            kind: TokenKind::NumberLiteral(num_part),
            raw: raw.to_string(),
            line,
        });
        return;
    }

    // ADR-0024: 助詞切り出し機構。既知の助詞で終わる語幹から助詞を独立トークンとして分離する。
    if let Some((stem, particle)) = split_trailing_particle(raw) {
        let normalized_stem = normalize_word(stem);
        tokens.push(Token {
            kind: TokenKind::Word(normalized_stem),
            raw: stem.to_string(),
            line,
        });
        tokens.push(Token {
            kind: TokenKind::Word(particle.to_string()),
            raw: particle.to_string(),
            line,
        });
        return;
    }

    let normalized = normalize_word(raw);
    tokens.push(Token {
        kind: TokenKind::Word(normalized),
        raw: raw.to_string(),
        line,
    });
}

/// 単語トークンの正規化（幅・大文字小文字の正規化 → 送り仮名除去）を行う。
pub(crate) fn normalize_word(raw: &str) -> String {
    let width_normalized = normalize_width_and_case(raw);
    normalize_okurigana(&width_normalized)
}

/// `buf` に溜まっている生トークンをトークン列へ確定させ、バッファを空にする。
fn flush_word(buf: &mut String, start_line: usize, tokens: &mut Vec<Token>) {
    if buf.is_empty() {
        return;
    }
    classify_and_push(buf, start_line, tokens);
    buf.clear();
}

/// `chars[i..]` が `pat` から始まっているかどうかを判定する。
fn starts_with_at(chars: &[char], i: usize, pat: &[char]) -> bool {
    if i + pat.len() > chars.len() {
        return false;
    }
    chars[i..i + pat.len()] == *pat
}

/// `chars[from..]` の中から `pat` を探し、見つかった開始位置を返す。
fn find_from(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (from..=chars.len() - pat.len()).find(|&start| chars[start..start + pat.len()] == *pat)
}

/// `//`・`／／`・`/*`・`／＊`・`*/`・`＊／` の構成要素としてのスラッシュ
/// （半角`/`・全角`／`）かどうかを判定する。
fn is_slash(c: char) -> bool {
    c == '/' || c == '\u{FF0F}'
}

/// `//`・`／／`・`/*`・`／＊`・`*/`・`＊／` の構成要素としてのアスタリスク
/// （半角`*`・全角`＊`）かどうかを判定する。
fn is_star(c: char) -> bool {
    c == '*' || c == '\u{FF0A}'
}

/// `chars[..idx]` を走査し、位置`idx`（0始まり）の行番号・列番号
/// （いずれも1始まり）を求める。エラー報告用に位置情報が必要になった
/// 時点で1度だけ呼び出される想定。
fn line_col_at(chars: &[char], idx: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for &c in &chars[..idx] {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// ソースコード文字列を字句解析し、トークン列を返す。
///
/// - 区切り文字（半角/全角スペース、タブ、半角/全角カンマ、半角/全角読点）で
///   単語を分割する（分かち書き）。
/// - `「...」` `"..."` は文字列リテラル、`'X'` は文字リテラルとして認識する。
/// - `※`・`//`・`／／`（半角/全角混在も可）〜行末、`コンパイル抑止。`〜
///   `コンパイル抑止終り。` は行コメントとして除去する。
/// - `/*`・`／＊`（半角/全角混在も可）〜`*/`・`＊／`はブロックコメントとして
///   除去する。ネスト可能で、対応する終了記号が見つかるまで（ネストの
///   深さが0に戻るまで）読み飛ばす。ファイル末尾まで閉じられなかった
///   場合は`LexError`を返す。
/// - `（...）` `(...)` は、直前に区切り文字を挟む場合はコメントとして除去し、
///   直前の語などに空白なしで隣接する場合は添字アクセス糖衣構文として
///   `OpenParen`/`CloseParen` トークンを生成する（中身は通常どおり字句解析する）。
/// - 単語の先頭が数値パターンの場合、数値部分を `NumberLiteral` として切り出す。
///   数値に空白なしで隣接する残り部分（助数詞等）はADR-0002の規定により
///   数値トークンへ完全に畳み込まれ、独立したトークンとしては現れない
///   （例: `５６０円` → `NumberLiteral("５６０")` のみ）。
/// - `。` は常に単独の `Word("。")` トークンとして切り出す。
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();

    let block_comment_open: Vec<char> = "コンパイル抑止。".chars().collect();
    let block_comment_close: Vec<char> = "コンパイル抑止終り。".chars().collect();

    let mut i = 0usize;
    let mut line = 1usize;
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut buf_start_line = line;
    // 添字アクセス糖衣構文として開かれた丸括弧の、対応する閉じ文字のスタック。
    let mut open_paren_stack: Vec<char> = Vec::new();

    while i < n {
        let c = chars[i];

        // ブロックコメント: コンパイル抑止。 〜 コンパイル抑止終り。
        if starts_with_at(&chars, i, &block_comment_open) {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let search_from = i + block_comment_open.len();
            let skip_to = match find_from(&chars, search_from, &block_comment_close) {
                Some(end_idx) => end_idx + block_comment_close.len(),
                None => n,
            };
            for &ch in &chars[i..skip_to] {
                if ch == '\n' {
                    line += 1;
                }
            }
            i = skip_to;
            buf_start_line = line;
            continue;
        }

        // 行コメント: ※ 〜 行末
        if c == '※' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            buf_start_line = line;
            continue;
        }

        // ブロックコメント: /* 〜 */ （／＊・＊／との半角/全角混在も可、ネスト対応）
        if is_slash(c) && i + 1 < n && is_star(chars[i + 1]) {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let comment_start = i;
            let mut depth = 1u32;
            let mut j = i + 2;
            while j < n && depth > 0 {
                if is_slash(chars[j]) && j + 1 < n && is_star(chars[j + 1]) {
                    depth += 1;
                    j += 2;
                    continue;
                }
                if is_star(chars[j]) && j + 1 < n && is_slash(chars[j + 1]) {
                    depth -= 1;
                    j += 2;
                    continue;
                }
                if chars[j] == '\n' {
                    line += 1;
                }
                j += 1;
            }
            if depth > 0 {
                let (err_line, err_col) = line_col_at(&chars, comment_start);
                return Err(LexError {
                    message: "ブロックコメントが閉じられていません".to_string(),
                    line: err_line,
                    column: err_col,
                });
            }
            i = j;
            buf_start_line = line;
            continue;
        }

        // 行コメント: // 〜 行末（／／との半角/全角混在も可）
        if is_slash(c) && i + 1 < n && is_slash(chars[i + 1]) {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            i += 2;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            buf_start_line = line;
            continue;
        }

        // 文の区切り: 。（常に単独のトークンとして切り出す）
        if c == '。' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            tokens.push(Token {
                kind: TokenKind::Word("。".to_string()),
                raw: "。".to_string(),
                line,
            });
            i += 1;
            buf_start_line = line;
            continue;
        }

        // 丸括弧: 直前が区切り文字なしで語などに隣接する場合は添字アクセス
        // 糖衣構文の開き括弧、それ以外（空白等で区切られている場合）は
        // コメントとして中身ごと読み飛ばす。
        if c == '（' || c == '(' {
            let adjacent = i > 0 && !is_delimiter(chars[i - 1]);
            let close = if c == '（' { '）' } else { ')' };

            if adjacent {
                flush_word(&mut buf, buf_start_line, &mut tokens);
                open_paren_stack.push(close);
                tokens.push(Token {
                    kind: TokenKind::OpenParen,
                    raw: c.to_string(),
                    line,
                });
                i += 1;
                buf_start_line = line;
                continue;
            }

            flush_word(&mut buf, buf_start_line, &mut tokens);
            i += 1;
            while i < n && chars[i] != close {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            if i < n {
                i += 1; // 閉じ括弧を読み飛ばす
            }
            buf_start_line = line;
            continue;
        }

        // 添字アクセス糖衣構文の閉じ括弧（対応する開き括弧がスタックにある場合のみ）。
        if (c == '）' || c == ')') && open_paren_stack.last() == Some(&c) {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            open_paren_stack.pop();
            tokens.push(Token {
                kind: TokenKind::CloseParen,
                raw: c.to_string(),
                line,
            });
            i += 1;
            buf_start_line = line;
            continue;
        }

        // ワード名クォート: 『...』 または “...” (ADR-0032)
        if c == '『' || c == '“' || c == '\u{201C}' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let start_line = line;
            let close = if c == '『' { '』' } else { '”' };
            let open_quote_str = c.to_string();
            let mut content = String::new();
            i += 1;
            while i < n && chars[i] != close && (close != '”' || chars[i] != '“') {
                if chars[i] == '\n' {
                    line += 1;
                }
                content.push(chars[i]);
                i += 1;
            }
            let raw = if i < n {
                let close_char = chars[i];
                i += 1;
                format!("{open_quote_str}{content}{close_char}")
            } else {
                format!("{open_quote_str}{content}")
            };
            let normalized = normalize_word(&content);
            tokens.push(Token {
                kind: TokenKind::Word(normalized),
                raw,
                line: start_line,
            });
            buf_start_line = line;
            continue;
        }

        // 文字列リテラル: 「...」
        if c == '「' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let start_line = line;
            let mut content = String::new();
            i += 1;
            while i < n && chars[i] != '」' {
                if chars[i] == '\n' {
                    line += 1;
                }
                content.push(chars[i]);
                i += 1;
            }
            let raw = format!("「{}」", content);
            if i < n {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::StringLiteral(content),
                raw,
                line: start_line,
            });
            buf_start_line = line;
            continue;
        }

        // 文字列リテラル: "..." または ＂...＂
        if c == '"' || c == '\u{FF02}' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let start_line = line;
            let quote = c;
            let mut content = String::new();
            i += 1;
            while i < n && chars[i] != quote {
                if chars[i] == '\n' {
                    line += 1;
                }
                content.push(chars[i]);
                i += 1;
            }
            let raw = format!("{quote}{content}{quote}");
            if i < n {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::StringLiteral(content),
                raw,
                line: start_line,
            });
            buf_start_line = line;
            continue;
        }

        // 文字リテラル: 'X' または ＇X＇
        if (c == '\'' || c == '\u{FF07}') && i + 2 < n && chars[i + 2] == c && chars[i + 1] != '\n'
        {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let ch = chars[i + 1];
            let raw: String = chars[i..=i + 2].iter().collect();
            tokens.push(Token {
                kind: TokenKind::CharLiteral(ch),
                raw,
                line,
            });
            i += 3;
            buf_start_line = line;
            continue;
        }

        // 区切り文字
        if is_delimiter(c) {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            if c == '\n' {
                line += 1;
            }
            i += 1;
            buf_start_line = line;
            continue;
        }

        // 通常文字: 生トークンバッファへ蓄積
        if buf.is_empty() {
            buf_start_line = line;
        }
        buf.push(c);
        i += 1;
    }

    flush_word(&mut buf, buf_start_line, &mut tokens);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(tokens: &[Token]) -> Vec<&str> {
        tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Word(w) => Some(w.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn splits_on_various_delimiters() {
        let tokens = tokenize("あ　い,う，え､お、か").unwrap();
        let ws = words(&tokens);
        assert_eq!(ws, vec!["あ", "い", "う", "え", "お", "か"]);
    }

    #[test]
    fn okurigana_variants_normalize_to_same_word() {
        for input in ["反応し", "反応する", "反応させる"] {
            let tokens = tokenize(input).unwrap();
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::Word("反応".to_string()));
        }
    }

    #[test]
    fn all_hiragana_words_are_preserved() {
        for input in ["ならば", "つぎに", "さもなければ"] {
            let tokens = tokenize(input).unwrap();
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::Word(input.to_string()));
        }
    }

    #[test]
    fn unsegmented_long_string_is_a_single_word() {
        let tokens = tokenize("赤い色で表示する").unwrap();
        assert_eq!(tokens.len(), 1);
        match &tokens[0].kind {
            TokenKind::Word(w) => assert_eq!(w, "赤色表示"),
            other => panic!("expected Word, got {other:?}"),
        }
    }

    #[test]
    fn string_literal_with_kagi_brackets() {
        let tokens = tokenize("「こんにちは。」を　表示する").unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("こんにちは。".to_string())
        );
        assert_eq!(tokens[1].kind, TokenKind::Word("を".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Word("表示".to_string()));
    }

    #[test]
    fn string_literal_with_double_quotes() {
        let tokens = tokenize(r#""hello world""#).unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("hello world".to_string())
        );
    }

    #[test]
    fn char_literal() {
        let tokens = tokenize("'A'").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::CharLiteral('A'));
    }

    #[test]
    fn number_literal_suffix_is_folded_away_per_adr_0002() {
        // ADR-0002: 数値に空白なしで隣接する非数値部分（助数詞等）は数値
        // トークンに完全に畳み込まれ、独立したトークン・ワードとして
        // 一切現れない（「円」は消える）。
        let tokens = tokenize("５６０円を　売り上げに　入れ").unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::NumberLiteral("５６０".to_string())
        );
        assert!(!tokens
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Word(w) if w == "円")));
        // 「円を」は数値に空白なしで隣接しているため、「を」も含めて
        // まるごと数値トークンへ畳み込まれ、消える。
        assert_eq!(words(&tokens), vec!["売上", "入"]);
    }

    #[test]
    fn counter_word_adjacent_to_number_is_folded_not_a_separate_word() {
        // issue #27: 「1つ」の「つ」が独立ワードとして辞書引きされ、
        // 未定義ワードエラーになってはならない。
        let tokens = tokenize("1つ　摘み").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::NumberLiteral("1".to_string()));
        assert_eq!(words(&tokens), vec!["摘"]);
    }

    #[test]
    fn counter_word_separated_by_space_stays_an_independent_word() {
        // ADR-0002: 空白を挟めば畳み込まれず、独立したワードとして残る。
        let tokens = tokenize("1 番目").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::NumberLiteral("1".to_string()));
        assert_eq!(words(&tokens), vec!["番目"]);
    }

    #[test]
    fn plain_number_literal() {
        let tokens = tokenize("-1.23E-2").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0].kind,
            TokenKind::NumberLiteral("-1.23E-2".to_string())
        );
    }

    #[test]
    fn parenthetical_comment_is_removed() {
        let tokens =
            tokenize("「こんにちは。」を 表示すること。 　　（これは暫定的な表示）").unwrap();
        for t in &tokens {
            match &t.kind {
                TokenKind::Word(w) => assert!(!w.contains("暫定的")),
                TokenKind::StringLiteral(s) => assert!(!s.contains("暫定的")),
                _ => {}
            }
        }
    }

    #[test]
    fn halfwidth_paren_comment_is_removed() {
        let tokens = tokenize("表示する (これはコメント)").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("表示".to_string()));
    }

    #[test]
    fn line_comment_runs_to_end_of_line() {
        let tokens = tokenize("表示する ※ ここはコメント\n実行する").unwrap();
        let ws = words(&tokens);
        assert_eq!(ws, vec!["表示", "実行"]);
    }

    #[test]
    fn block_comment_is_removed() {
        let tokens =
            tokenize("表示する コンパイル抑止。 これは無効 コンパイル抑止終り。 実行する").unwrap();
        let ws = words(&tokens);
        assert_eq!(ws, vec!["表示", "実行"]);
    }

    #[test]
    fn keyword_suffix_with_no_space_is_absorbed_as_okurigana() {
        // 構造キーワードの前には必ず空白が必要というルールにより、
        // 空白なしの「挨拶するとは」は「とは」が送り仮名として吸収され、
        // 単一の Word("挨拶") になるのが正しい挙動。
        let tokens = tokenize("挨拶するとは").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("挨拶".to_string()));
    }

    #[test]
    fn standalone_keyword_is_not_split_further() {
        let tokens = tokenize("とは").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("とは".to_string()));
    }

    #[test]
    fn wa_particle_with_leading_space_stays_a_separate_token() {
        // 構造キーワードの前には空白が必要なので、「X は」のように空白を
        // 挟んで書けば、送り仮名除去の対象にならず別トークンとして残る。
        let tokens = tokenize("Xは 変数").unwrap();
        assert_eq!(words(&tokens), vec!["x", "変数"]);

        let tokens = tokenize("X は 変数").unwrap();
        assert_eq!(words(&tokens), vec!["x", "は", "変数"]);
    }

    #[test]
    fn period_is_always_its_own_token() {
        let tokens = tokenize("すること。").unwrap();
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Word("すること".to_string()),
                &TokenKind::Word("。".to_string()),
            ]
        );
    }

    #[test]
    fn paren_adjacent_to_preceding_word_is_subscript_access() {
        let tokens = tokenize("売り上げ（1）").unwrap();
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::Word("売上".to_string()),
                &TokenKind::OpenParen,
                &TokenKind::NumberLiteral("1".to_string()),
                &TokenKind::CloseParen,
            ]
        );
    }

    #[test]
    fn chained_subscript_access_produces_two_bracket_pairs() {
        let tokens = tokenize("ダンジョンマップ（X軸座標）（Y座標）").unwrap();
        let kinds: Vec<&TokenKind> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Word("ダンジョンマップ".to_string()),
                &TokenKind::OpenParen,
                &TokenKind::Word("x軸座標".to_string()),
                &TokenKind::CloseParen,
                &TokenKind::OpenParen,
                &TokenKind::Word("y座標".to_string()),
                &TokenKind::CloseParen,
            ]
        );
    }

    #[test]
    fn paren_preceded_by_space_is_still_a_comment_not_subscript() {
        let tokens = tokenize("実行する （これは説明）").unwrap();
        let ws = words(&tokens);
        assert_eq!(ws, vec!["実行"]);
        assert!(!tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::OpenParen | TokenKind::CloseParen)));
    }

    #[test]
    fn line_numbers_are_tracked() {
        let tokens = tokenize("あ\nい\nう").unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[2].line, 3);
    }

    #[test]
    fn raw_form_is_preserved_before_normalization() {
        let tokens = tokenize("ﾊﾞｽﾞる").unwrap();
        match &tokens[0].kind {
            TokenKind::Word(w) => assert_eq!(w, "バズ"),
            other => panic!("expected Word, got {other:?}"),
        }
        assert_eq!(tokens[0].raw, "ﾊﾞｽﾞる");
    }

    #[test]
    fn slash_line_comment_is_equivalent_to_mind_comment() {
        let a = tokenize("5 を X に いれる ※ Mindスタイル").unwrap();
        let b = tokenize("5 を X に いれる // Cスタイル").unwrap();
        let c = tokenize("5 を X に いれる ／／ 全角Cスタイル").unwrap();
        let expected = vec![
            TokenKind::NumberLiteral("5".to_string()),
            TokenKind::Word("を".to_string()),
            TokenKind::Word("x".to_string()),
            TokenKind::Word("に".to_string()),
            TokenKind::Word("いれる".to_string()),
        ];
        for tokens in [a, b, c] {
            let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
            assert_eq!(kinds, expected);
        }
    }

    #[test]
    fn mixed_width_slash_line_comment_is_recognized() {
        let tokens = tokenize("表示する /／ 半角全角混在\n実行する").unwrap();
        assert_eq!(words(&tokens), vec!["表示", "実行"]);
    }

    #[test]
    fn nested_block_comments_are_fully_removed() {
        let tokens = tokenize(
            "処理 とは\n    /* これは\n       /* ネストした */\n       コメント全体 */\n    なにかする\nこと。",
        )
        .unwrap();
        for t in &tokens {
            if let TokenKind::Word(w) = &t.kind {
                assert!(!w.contains("これ"));
                assert!(!w.contains("ネスト"));
                assert!(!w.contains("コメント全体"));
            }
        }
        assert!(words(&tokens).contains(&"なにかする"));
    }

    #[test]
    fn fullwidth_block_comment_marker_is_removed() {
        let tokens = tokenize("／＊ 全角開始、半角終了 */\nなにかする").unwrap();
        assert_eq!(words(&tokens), vec!["なにかする"]);
    }

    #[test]
    fn unterminated_block_comment_is_a_lex_error() {
        let err = tokenize("処理 とは\n    /* 閉じ忘れ\n    なにかする\nこと。").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn slash_inside_string_literal_is_not_a_comment() {
        let tokens = tokenize("「// これはコメントではない」を 表示する").unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("// これはコメントではない".to_string())
        );
        assert_eq!(words(&tokens), vec!["を", "表示"]);
    }
}
