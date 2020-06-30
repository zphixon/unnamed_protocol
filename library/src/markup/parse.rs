use crate::{AddMsg, FroggiError, ParseError};

use super::scan::{Scanner, Token, TokenKind};
use super::{InlineStyle, ItemPayload, Page, PageItem, PageStyle, WithArg, WithoutArg};

/// Parse some data into a Page.
pub fn parse(data: &str) -> Result<Page<'_>, Vec<FroggiError>> {
    let mut errors = Vec::new();
    let mut items = Vec::new();
    let mut page_styles = Vec::new();

    let mut scanner = Scanner::new(data);
    while scanner.peek_token(0)?.kind() != TokenKind::End {
        match scanner.peek_token(0)?.kind() {
            // there should only be a single page-level style element
            TokenKind::LeftBrace if page_styles.is_empty() => match parse_page_styles(&mut scanner)
            {
                Ok(styles) => page_styles = styles,
                Err(error) => errors.push(error),
            },

            TokenKind::LeftParen => match parse_item(&mut scanner) {
                Ok(item) => {
                    items.push(item);
                }
                Err(error) => {
                    errors.push(error);
                }
            },

            _ => {
                errors.push(FroggiError::parse(
                    ParseError::ExpectedItem {
                        got: scanner.peek_token(0)?.clone_lexeme(),
                    },
                    scanner.peek_token(0)?.line(),
                ));
                scanner.next_token()?;
            }
        }
    }

    if errors.is_empty() {
        Ok(Page { page_styles, items })
    } else {
        Err(errors)
    }
}

// consume top-level page style
fn parse_page_styles<'a>(scanner: &mut Scanner<'a>) -> Result<Vec<PageStyle<'a>>, FroggiError> {
    // parse outer list of rules
    let left_brace = consume(scanner, TokenKind::LeftBrace)?;

    let mut page_styles = Vec::new();

    while scanner.peek_token(0)?.kind() != TokenKind::RightBrace {
        // parse one single rule
        consume(scanner, TokenKind::LeftParen)
            .msg_str("expected style rules inside page style item")?;

        // name of the rule
        let selector = consume_selector(scanner)?;

        // styles that belong to the rule
        let mut styles = Vec::new();
        while scanner.peek_token(0)?.kind() != TokenKind::RightParen {
            let token = scanner.next_token()?;
            match token.kind() {
                TokenKind::Identifier => {
                    styles.push(InlineStyle::WithoutArg(WithoutArg { name: token }));
                }

                TokenKind::LeftParen => {
                    styles.push(InlineStyle::WithArg(WithArg {
                        name: consume(scanner, TokenKind::Identifier)
                            .msg_str("expected a built-in style rule")?,
                        arg: consume(scanner, TokenKind::String)
                            .msg_str("expected an argument to the built-in style rule")?,
                    }));
                    consume(scanner, TokenKind::RightParen)
                        .msg_str("style rules only take one argument")?;
                }

                _ => {
                    return Err(FroggiError::parse(
                        ParseError::ExpectedStyle {
                            got: token.clone_lexeme(),
                        },
                        token.line(),
                    ))
                    .msg_str("expected a style rule")
                }
            }
        }

        page_styles.push(PageStyle { selector, styles });
        consume(scanner, TokenKind::RightParen).msg_str("end of the style rule")?;
    }

    consume(scanner, TokenKind::RightBrace).msg(format!(
        "unbalanced braces starting on line {}",
        left_brace.line()
    ))?;

    Ok(page_styles)
}

// parse some normal page item
fn parse_item<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let left_paren = consume(scanner, TokenKind::LeftParen)?;

    let result = match scanner.peek_token(0)?.kind() {
        TokenKind::Blob => parse_blob(scanner)?,
        TokenKind::Link => parse_link(scanner)?,
        TokenKind::Anchor => parse_anchor(scanner)?,
        TokenKind::Text => parse_text(scanner)?,
        TokenKind::VBox => parse_vbox(scanner)?,
        TokenKind::Box => parse_box(scanner)?,
        _ => parse_implicit_text(scanner)?,
    };

    consume(scanner, TokenKind::RightParen).msg(format!(
        "unbalanced parens starting on line {}",
        left_paren.line()
    ))?;

    Ok(result)
}

fn parse_blob<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::Blob)?;
    let name = consume(scanner, TokenKind::String)?;

    let inline_styles = parse_inline_styles(scanner)?;
    let payload = ItemPayload::Blob {
        name,
        alt: collect_text(scanner)?,
    };

    Ok(PageItem {
        builtin,
        inline_styles,
        payload,
    })
}

fn parse_link<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::Link)?;
    let link = consume(scanner, TokenKind::String)?;

    let inline_styles = parse_inline_styles(scanner)?;
    let payload = ItemPayload::Link {
        link,
        text: collect_text(scanner)?,
    };

    Ok(PageItem {
        builtin,
        inline_styles,
        payload,
    })
}

fn parse_anchor<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::Anchor)?;
    let anchor = consume(scanner, TokenKind::String)?;
    let payload = ItemPayload::Anchor { anchor };
    Ok(PageItem {
        builtin,
        inline_styles: Vec::new(),
        payload,
    })
}

fn parse_text<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::Text)?;
    let inline_styles = parse_inline_styles(scanner)?;
    let text = collect_text(scanner)?;

    Ok(PageItem {
        builtin,
        inline_styles,
        payload: ItemPayload::Text { text },
    })
}

fn parse_vbox<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::VBox)?;
    let inline_styles = parse_inline_styles(scanner)?;
    let mut children = Vec::new();

    while scanner.peek_token(0)?.kind() != TokenKind::RightParen {
        children.push(parse_item(scanner)?);
    }

    Ok(PageItem {
        builtin,
        inline_styles,
        payload: ItemPayload::Children {
            children,
            line: builtin.line(),
        },
    })
}

fn parse_box<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let builtin = consume(scanner, TokenKind::Box)?;
    let inline_styles = parse_inline_styles(scanner)?;
    let mut children = Vec::new();

    while scanner.peek_token(0)?.kind() != TokenKind::RightParen {
        children.push(parse_item(scanner)?);
    }

    Ok(PageItem {
        builtin,
        inline_styles,
        payload: ItemPayload::Children {
            children,
            line: builtin.line(),
        },
    })
}

fn parse_implicit_text<'a>(scanner: &mut Scanner<'a>) -> Result<PageItem<'a>, FroggiError> {
    let implicit = Token::new(TokenKind::ImplicitText, scanner.peek_token(0)?.line(), "");
    let inline_styles = parse_inline_styles(scanner)?;
    let text = collect_text(scanner)?;

    Ok(PageItem {
        builtin: implicit,
        inline_styles,
        payload: ItemPayload::Text { text },
    })
}

fn parse_inline_styles<'a>(scanner: &mut Scanner<'a>) -> Result<Vec<InlineStyle<'a>>, FroggiError> {
    let mut inline_styles = Vec::new();
    if scanner.peek_token(0)?.kind() == TokenKind::LeftBrace {
        consume(scanner, TokenKind::LeftBrace)?;

        while scanner.peek_token(0)?.kind() != TokenKind::RightBrace {
            let token = scanner.next_token()?;
            match token.kind() {
                TokenKind::Identifier => {
                    inline_styles.push(InlineStyle::WithoutArg(WithoutArg { name: token }));
                }

                TokenKind::LeftParen => {
                    inline_styles.push(InlineStyle::WithArg(WithArg {
                        name: consume(scanner, TokenKind::Identifier)
                            .msg_str("expected some built-in style name")?,
                        arg: consume(scanner, TokenKind::String)
                            .msg_str("expected an argument to the built-in style")?,
                    }));
                    consume(scanner, TokenKind::RightParen)
                        .msg_str("styles only take one argument")?;
                }

                _ => {
                    return Err(FroggiError::parse(
                        ParseError::ExpectedStyle {
                            got: token.clone_lexeme(),
                        },
                        token.line(),
                    ))
                    .msg_str("expected a style rule in the list of inline style rules")
                }
            }
        }

        consume(scanner, TokenKind::RightBrace).msg_str("expected the end of the inline style")?;
    }

    Ok(inline_styles)
}

fn collect_text<'a>(scanner: &mut Scanner<'a>) -> Result<Vec<Token<'a>>, FroggiError> {
    let mut text = Vec::new();

    while scanner.peek_token(0)?.kind() != TokenKind::RightParen {
        text.push(consume(scanner, TokenKind::String)?);
    }

    Ok(text)
}

fn consume_selector<'a>(scanner: &mut Scanner<'a>) -> Result<Token<'a>, FroggiError> {
    let token = scanner.next_token()?;
    match token.kind() {
        TokenKind::Identifier
        | TokenKind::Link
        | TokenKind::Box
        | TokenKind::VBox
        | TokenKind::Text => Ok(token),
        _ => Err(FroggiError::parse(
            ParseError::UnexpectedToken {
                expected: TokenKind::Identifier,
                got: token.clone_lexeme(),
            },
            token.line(),
        ))
        .msg_str("selectors must be either built-in items or links, or user-defined selectors"),
    }
}

fn consume<'a>(scanner: &mut Scanner<'a>, kind: TokenKind) -> Result<Token<'a>, FroggiError> {
    let token = scanner.next_token()?;

    if token.kind() == kind {
        Ok(token)
    } else {
        Err(FroggiError::parse(
            ParseError::UnexpectedToken {
                expected: kind,
                got: token.clone_lexeme(),
            },
            token.line(),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn anchor() {
        let sample = r#"(# "something")"#;
        let page = parse(sample).unwrap();
        match page.items[0].payload {
            ItemPayload::Anchor { anchor } => assert_eq!(anchor.lexeme(), "something"),
            _ => panic!(),
        }
    }

    #[test]
    fn references() {
        let sample = r#"(& "image.jpg" {user-style (fg "30300") serif} "with alt" " text")"#;
        parse(sample).unwrap();

        let sample = r#"(& "somewhere")"#;
        parse(sample).unwrap();
    }

    #[test]
    fn links() {
        let sample =
            r#"(^ "frgi://www.lipsum.com/" {footnote (fill "20")} "from frgi://www.lipsum.com/")"#;
        parse(sample).unwrap();

        let sample = r#"(^ "frgi://www.lipsum.com/" {serif })"#;
        parse(sample).unwrap();

        let sample = r#"(^ "frgi://www.lipsum.com/")"#;
        parse(sample).unwrap();
    }

    #[test]
    fn parent_style_missing_arg() {
        let item = "(this {style (missing)} (\"multiple children?\") (doesnt-work \"why\"))";
        assert!(parse(item).is_err());
    }

    #[test]
    fn child_style_missing_arg() {
        let item = "(this (doesnt-work {style (missing)} \"why\"))";
        assert!(parse(item).is_err());
    }

    #[test]
    fn page_style_missing_arg() {
        let style = "{(item style (missing))}";
        assert!(parse(style).is_err());
    }

    #[test]
    fn item_inline_style_missing_arg() {
        let item = "(item {style (missing)} \"arg\")";
        let mut scanner = Scanner::new(item);
        assert!(parse_item(&mut scanner).is_err());
    }

    #[test]
    fn inline_style_missing_arg() {
        let style = "{inline-style (something)}";
        let mut scanner = Scanner::new(style);
        assert!(parse_inline_styles(&mut scanner).is_err());
    }

    #[test]
    fn well_formed_page_item() {
        let item =
            "(box {user-style inline-style (with \"args\")} (\"children\") ({with} \"style\"))";
        parse(item).unwrap();
    }

    #[test]
    fn ill_formed_page_styles() {
        use crate::markup::scan::Scanner;

        let style = "{";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "{text) serif}";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "{(text) serif}";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "{() (style)}";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "{( (style)}";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "{ (style))}";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());

        let style = "";
        let mut scanner = Scanner::new(style);
        assert!(parse_page_styles(&mut scanner).is_err());
    }

    #[test]
    fn well_formed_page_style() {
        let style = "{(text serif)(footnote underline (zip \"90210\"))}";
        let mut scanner = crate::markup::scan::Scanner::new(style);
        let style = parse_page_styles(&mut scanner).unwrap();
        assert_eq!(
            style,
            vec![
                PageStyle {
                    selector: Token::new(TokenKind::Text, 1, "text"),
                    styles: vec![InlineStyle::WithoutArg(WithoutArg {
                        name: Token::new(TokenKind::Identifier, 1, "serif",),
                    }),]
                },
                PageStyle {
                    selector: Token::new(TokenKind::Identifier, 1, "footnote"),
                    styles: vec![
                        InlineStyle::WithoutArg(WithoutArg {
                            name: Token::new(TokenKind::Identifier, 1, "underline")
                        }),
                        InlineStyle::WithArg(WithArg {
                            name: Token::new(TokenKind::Identifier, 1, "zip"),
                            arg: Token::new(TokenKind::String, 1, "\"90210\"")
                        })
                    ]
                }
            ]
        );
    }

    #[test]
    fn test_markup() {
        let sample = include_str!("../../../server/pages/test_markup.fml");
        parse(sample).unwrap();
    }
}
