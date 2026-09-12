//! 実装指示書に記載された受け入れテストケース（テストケース1〜6）。

use umor::{tokenize, TokenKind};

#[test]
fn case1_basic_wakachigaki_and_okurigana_removal() {
    let tokens = tokenize("「こんにちは。」を　表示する").unwrap();
    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::StringLiteral("こんにちは。".to_string()),
            TokenKind::Word("を".to_string()),
            TokenKind::Word("表示".to_string()),
        ]
    );
}

#[test]
fn case2_okurigana_variants_normalize_to_same_word() {
    for input in ["反応し", "反応する", "反応させる"] {
        let tokens = tokenize(input).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("反応".to_string()));
    }
}

#[test]
fn case3_all_hiragana_words_are_not_stripped() {
    assert_eq!(
        tokenize("ならば").unwrap()[0].kind,
        TokenKind::Word("ならば".to_string())
    );
    assert_eq!(
        tokenize("つぎに").unwrap()[0].kind,
        TokenKind::Word("つぎに".to_string())
    );
    assert_eq!(
        tokenize("さもなければ").unwrap()[0].kind,
        TokenKind::Word("さもなければ".to_string())
    );
}

#[test]
fn case4_number_and_josuushi() {
    let tokens = tokenize("５６０円を　売り上げに　入れ").unwrap();
    assert_eq!(
        tokens[0].kind,
        TokenKind::NumberLiteral("５６０".to_string())
    );
    // 数値部分がどのトークンにも Word として混入していないこと。
    assert!(!tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Word(w) if w.chars().any(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(&c)))));
}

#[test]
fn case5_parenthetical_comment_is_excluded() {
    let tokens = tokenize("「こんにちは。」を 表示すること。 　　（これは暫定的な表示）").unwrap();
    for t in &tokens {
        match &t.kind {
            TokenKind::Word(w) => assert!(!w.contains("暫定的")),
            TokenKind::StringLiteral(s) => assert!(!s.contains("暫定的")),
            TokenKind::NumberLiteral(s) => assert!(!s.contains("暫定的")),
            TokenKind::CharLiteral(_) => {}
            TokenKind::OpenParen | TokenKind::CloseParen => {}
        }
    }
}

#[test]
fn case6_unsegmented_long_word_stays_one_token() {
    let tokens = tokenize("赤い色で表示する").unwrap();
    assert_eq!(tokens.len(), 1);
    assert!(matches!(&tokens[0].kind, TokenKind::Word(_)));
}

#[test]
fn adr0024_hiragana_word_particle_splitting() {
    // 純ひらがな語に助詞が膠着した場合、語幹と助詞に分離される。
    let tokens = tokenize("ほにに ほにを ほには ほにが ほにへ ほにと ほにで ほにも ほにや ほにから ほにまで ほにより").unwrap();
    let words: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Word(w) => Some(w.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        words,
        vec![
            "ほに", "に",
            "ほに", "を",
            "ほに", "は",
            "ほに", "が",
            "ほに", "へ",
            "ほに", "と",
            "ほに", "で",
            "ほに", "も",
            "ほに", "や",
            "ほに", "から",
            "ほに", "まで",
            "ほに", "より",
        ]
    );
}

#[test]
fn adr0024_protected_keywords_are_not_split() {
    // 助詞と同じ文字列で終わる保護キーワードが誤分割されないこと。
    for kw in [
        "ここから",
        "つぎに",
        "こと",
        "とは",
        "ならば",
        "そうでなければ",
        "さよなら",
        "または",
    ] {
        let tokens = tokenize(kw).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word(kw.to_string()));
    }
}

#[test]
fn adr0032_quoted_identifiers_with_kagi_and_curly_quotes() {
    // 二重カギ括弧『』によるクォート（ADR-0032）
    let tokens = tokenize("『わに』に 『ほげ』を").unwrap();
    let words: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Word(w) => Some(w.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(words, vec!["わに", "に", "ほげ", "を"]);

    // 全角二重カーリー引用符“”によるクォート（ADR-0032）
    let tokens = tokenize("“わに”に “ほげ”を").unwrap();
    let words: Vec<&str> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Word(w) => Some(w.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(words, vec!["わに", "に", "ほげ", "を"]);

    // 単体のクォート識別子は通常の識別子と一致すること
    let tokens = tokenize("『わに』").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::Word("わに".to_string()));
}

