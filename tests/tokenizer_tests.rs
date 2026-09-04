//! 実装指示書に記載された受け入れテストケース（テストケース1〜6）。

use umor::{tokenize, TokenKind};

#[test]
fn case1_basic_wakachigaki_and_okurigana_removal() {
    let tokens = tokenize("「こんにちは。」を　表示する");
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
        let tokens = tokenize(input);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Word("反応".to_string()));
    }
}

#[test]
fn case3_all_hiragana_words_are_not_stripped() {
    assert_eq!(
        tokenize("ならば")[0].kind,
        TokenKind::Word("ならば".to_string())
    );
    assert_eq!(
        tokenize("つぎに")[0].kind,
        TokenKind::Word("つぎに".to_string())
    );
    assert_eq!(
        tokenize("さもなければ")[0].kind,
        TokenKind::Word("さもなければ".to_string())
    );
}

#[test]
fn case4_number_and_josuushi() {
    let tokens = tokenize("５６０円を　売り上げに　入れ");
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
    let tokens = tokenize("「こんにちは。」を 表示すること。 　　（これは暫定的な表示）");
    for t in &tokens {
        match &t.kind {
            TokenKind::Word(w) => assert!(!w.contains("暫定的")),
            TokenKind::StringLiteral(s) => assert!(!s.contains("暫定的")),
            TokenKind::NumberLiteral(s) => assert!(!s.contains("暫定的")),
            TokenKind::CharLiteral(_) => {}
        }
    }
}

#[test]
fn case6_unsegmented_long_word_stays_one_token() {
    let tokens = tokenize("赤い色で表示する");
    assert_eq!(tokens.len(), 1);
    assert!(matches!(&tokens[0].kind, TokenKind::Word(_)));
}
