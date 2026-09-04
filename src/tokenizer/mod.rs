//! Umorの字句解析器（Tokenizer）。
//!
//! ソースコードを「分かち書き」された単語や各種リテラルのトークン列へ変換する。
//! Mindの構文思想（分かち書き・送り仮名の無視）を踏襲しつつ、実装は
//! シンプルさを優先する。

mod kana_table;
mod normalize;
mod okurigana;

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

/// 生トークン（区切り文字を含まない1塊の文字列）を分類し、トークンとして積む。
///
/// 先頭が数値パターンで構成される場合、数値部分と残り部分（助数詞等）を
/// 分割して2つのトークンにする（例: 「５６０円」→ NumberLiteral("５６０") + Word("円")）。
fn classify_and_push(raw: &str, line: usize, tokens: &mut Vec<Token>) {
    let chars: Vec<char> = raw.chars().collect();
    let num_len = number_prefix_len(&chars);

    if num_len > 0 && contains_digit(&chars[..num_len]) {
        let num_part: String = chars[..num_len].iter().collect();
        tokens.push(Token {
            kind: TokenKind::NumberLiteral(num_part.clone()),
            raw: num_part,
            line,
        });

        if num_len < chars.len() {
            let rest: String = chars[num_len..].iter().collect();
            let normalized = normalize_word(&rest);
            tokens.push(Token {
                kind: TokenKind::Word(normalized),
                raw: rest,
                line,
            });
        }
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
fn normalize_word(raw: &str) -> String {
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

/// ソースコード文字列を字句解析し、トークン列を返す。
///
/// - 区切り文字（半角/全角スペース、タブ、半角/全角カンマ、半角/全角読点）で
///   単語を分割する（分かち書き）。
/// - `「...」` `"..."` は文字列リテラル、`'X'` は文字リテラルとして認識する。
/// - `（...）` `(...)` 、`※`〜行末、`コンパイル抑止。`〜`コンパイル抑止終り。` は
///   コメントとして除去する。
/// - 単語の先頭が数値パターンの場合、数値部分を `NumberLiteral` として切り出す。
pub fn tokenize(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();

    let block_comment_open: Vec<char> = "コンパイル抑止。".chars().collect();
    let block_comment_close: Vec<char> = "コンパイル抑止終り。".chars().collect();

    let mut i = 0usize;
    let mut line = 1usize;
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut buf_start_line = line;

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

        // 丸括弧コメント: （...） または (...)
        if c == '（' || c == '(' {
            flush_word(&mut buf, buf_start_line, &mut tokens);
            let close = if c == '（' { '）' } else { ')' };
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
    tokens
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
        let tokens = tokenize("あ　い,う，え､お、か");
        let ws = words(&tokens);
        assert_eq!(ws, vec!["あ", "い", "う", "え", "お", "か"]);
    }

    #[test]
    fn okurigana_variants_normalize_to_same_word() {
        for input in ["反応し", "反応する", "反応させる"] {
            let tokens = tokenize(input);
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::Word("反応".to_string()));
        }
    }

    #[test]
    fn all_hiragana_words_are_preserved() {
        for input in ["ならば", "つぎに", "さもなければ"] {
            let tokens = tokenize(input);
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::Word(input.to_string()));
        }
    }

    #[test]
    fn unsegmented_long_string_is_a_single_word() {
        let tokens = tokenize("赤い色で表示する");
        assert_eq!(tokens.len(), 1);
        match &tokens[0].kind {
            TokenKind::Word(w) => assert_eq!(w, "赤い色で表示"),
            other => panic!("expected Word, got {other:?}"),
        }
    }

    #[test]
    fn string_literal_with_kagi_brackets() {
        let tokens = tokenize("「こんにちは。」を　表示する");
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("こんにちは。".to_string())
        );
        assert_eq!(tokens[1].kind, TokenKind::Word("を".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Word("表示".to_string()));
    }

    #[test]
    fn string_literal_with_double_quotes() {
        let tokens = tokenize(r#""hello world""#);
        assert_eq!(
            tokens[0].kind,
            TokenKind::StringLiteral("hello world".to_string())
        );
    }

    #[test]
    fn char_literal() {
        let tokens = tokenize("'A'");
        assert_eq!(tokens[0].kind, TokenKind::CharLiteral('A'));
    }

    #[test]
    fn number_literal_prefix_is_split_from_word() {
        let tokens = tokenize("５６０円を　売り上げに　入れ");
        assert_eq!(tokens[0].kind, TokenKind::NumberLiteral("５６０".to_string()));
        assert!(tokens
            .iter()
            .all(|t| !matches!(&t.kind, TokenKind::Word(w) if w.contains('５'))));
    }

    #[test]
    fn plain_number_literal() {
        let tokens = tokenize("-1.23E-2");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::NumberLiteral("-1.23E-2".to_string()));
    }

    #[test]
    fn parenthetical_comment_is_removed() {
        let tokens = tokenize("「こんにちは。」を 表示すること。 　　（これは暫定的な表示）");
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
        let tokens = tokenize("表示する (これはコメント)");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("表示".to_string()));
    }

    #[test]
    fn line_comment_runs_to_end_of_line() {
        let tokens = tokenize("表示する ※ ここはコメント\n実行する");
        let ws = words(&tokens);
        assert_eq!(ws, vec!["表示", "実行"]);
    }

    #[test]
    fn block_comment_is_removed() {
        let tokens = tokenize("表示する コンパイル抑止。 これは無効 コンパイル抑止終り。 実行する");
        let ws = words(&tokens);
        assert_eq!(ws, vec!["表示", "実行"]);
    }

    #[test]
    fn line_numbers_are_tracked() {
        let tokens = tokenize("あ\nい\nう");
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[2].line, 3);
    }

    #[test]
    fn raw_form_is_preserved_before_normalization() {
        let tokens = tokenize("ﾊﾞｽﾞる");
        match &tokens[0].kind {
            TokenKind::Word(w) => assert_eq!(w, "バズ"),
            other => panic!("expected Word, got {other:?}"),
        }
        assert_eq!(tokens[0].raw, "ﾊﾞｽﾞる");
    }
}
