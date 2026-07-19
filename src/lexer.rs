use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Let,
    Mut,
    Copy,
    Move,
    Borrow,
    Type,
    Do,
    If,
    Else,
    Return,
    Throw,
    While,
    Loop,
    Break,
    Extend,
    Struct,
    Enum,
    Trait,
    Match,
    Try,
    True,
    False,
    Ident(String),
    Integer(i128),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Colon,
    Dot,
    Comma,
    Semicolon,
    Newline,
    Arrow,
    FatArrow,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    QuestionDot,
    QuestionQuestion,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for LexError {}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        index: 0,
        line: 1,
        column: 1,
        delimiter_depth: 0,
    };
    lexer.run()
}

struct Lexer {
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
    delimiter_depth: usize,
}

impl Lexer {
    fn run(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                self.logical_newline(&mut tokens);
                continue;
            }
            if ch.is_whitespace() {
                self.bump();
                continue;
            }
            if ch == '/' && self.peek_next() == Some('/') {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.bump();
                }
                continue;
            }
            if ch == '/' && self.peek_next() == Some('*') {
                self.block_comment(&mut tokens)?;
                continue;
            }

            let line = self.line;
            let column = self.column;
            let kind = if ch.is_ascii_digit() {
                self.number()?
            } else if ch == '_' || ch.is_alphabetic() {
                self.identifier()
            } else {
                self.bump();
                match ch {
                    '(' => {
                        self.delimiter_depth += 1;
                        TokenKind::LParen
                    }
                    ')' => {
                        self.delimiter_depth = self.delimiter_depth.saturating_sub(1);
                        TokenKind::RParen
                    }
                    '[' => {
                        self.delimiter_depth += 1;
                        TokenKind::LBracket
                    }
                    ']' => {
                        self.delimiter_depth = self.delimiter_depth.saturating_sub(1);
                        TokenKind::RBracket
                    }
                    '{' => TokenKind::LBrace,
                    '}' => TokenKind::RBrace,
                    ':' => TokenKind::Colon,
                    '.' => TokenKind::Dot,
                    ',' => TokenKind::Comma,
                    ';' => TokenKind::Semicolon,
                    '+' => TokenKind::Plus,
                    '*' => TokenKind::Star,
                    '%' => TokenKind::Percent,
                    '-' if self.take('>') => TokenKind::Arrow,
                    '-' => TokenKind::Minus,
                    '=' if self.take('=') => TokenKind::EqualEqual,
                    '=' if self.take('>') => TokenKind::FatArrow,
                    '=' => TokenKind::Equal,
                    '!' if self.take('=') => TokenKind::BangEqual,
                    '!' => TokenKind::Bang,
                    '<' if self.take('=') => TokenKind::LessEqual,
                    '<' => TokenKind::Less,
                    '>' if self.take('=') => TokenKind::GreaterEqual,
                    '>' => TokenKind::Greater,
                    '&' if self.take('&') => TokenKind::AndAnd,
                    '|' if self.take('|') => TokenKind::OrOr,
                    '?' if self.take('.') => TokenKind::QuestionDot,
                    '?' if self.take('?') => TokenKind::QuestionQuestion,
                    '/' => TokenKind::Slash,
                    _ => {
                        return Err(self.error(
                            format!("unexpected character `{ch}`"),
                            line,
                            column,
                        ));
                    }
                }
            };
            tokens.push(Token { kind, line, column });
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.line,
            column: self.column,
        });
        Ok(tokens)
    }

    fn logical_newline(&mut self, tokens: &mut Vec<Token>) {
        let line = self.line;
        let column = self.column;
        self.bump();

        let continued = tokens.last().is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Colon
                    | TokenKind::Dot
                    | TokenKind::Comma
                    | TokenKind::Arrow
                    | TokenKind::FatArrow
                    | TokenKind::Equal
                    | TokenKind::EqualEqual
                    | TokenKind::Bang
                    | TokenKind::BangEqual
                    | TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Percent
                    | TokenKind::Less
                    | TokenKind::LessEqual
                    | TokenKind::Greater
                    | TokenKind::GreaterEqual
                    | TokenKind::AndAnd
                    | TokenKind::OrOr
                    | TokenKind::QuestionDot
                    | TokenKind::QuestionQuestion
            )
        });

        if self.delimiter_depth == 0 && !continued {
            tokens.push(Token {
                kind: TokenKind::Newline,
                line,
                column,
            });
        }
    }

    fn block_comment(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        let start_line = self.line;
        let start_column = self.column;
        self.bump();
        self.bump();
        let mut depth = 1usize;

        while let Some(ch) = self.peek() {
            if ch == '/' && self.peek_next() == Some('*') {
                self.bump();
                self.bump();
                depth += 1;
            } else if ch == '*' && self.peek_next() == Some('/') {
                self.bump();
                self.bump();
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            } else if ch == '\n' {
                self.logical_newline(tokens);
            } else {
                self.bump();
            }
        }

        Err(self.error(
            "unterminated block comment".into(),
            start_line,
            start_column,
        ))
    }

    fn number(&mut self) -> Result<TokenKind, LexError> {
        let line = self.line;
        let column = self.column;
        let mut text = String::new();
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
            let c = self.bump().expect("peeked character exists");
            if c != '_' {
                text.push(c);
            }
        }
        text.parse::<i128>()
            .map(TokenKind::Integer)
            .map_err(|_| self.error("integer literal is too large".into(), line, column))
    }

    fn identifier(&mut self) -> TokenKind {
        let mut text = String::new();
        while self.peek().is_some_and(|c| c == '_' || c.is_alphanumeric()) {
            text.push(self.bump().expect("peeked character exists"));
        }
        match text.as_str() {
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "copy" => TokenKind::Copy,
            "move" => TokenKind::Move,
            "borrow" => TokenKind::Borrow,
            "type" => TokenKind::Type,
            "do" => TokenKind::Do,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "return" => TokenKind::Return,
            "throw" => TokenKind::Throw,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "extend" => TokenKind::Extend,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "trait" => TokenKind::Trait,
            "match" => TokenKind::Match,
            "try" => TokenKind::Try,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(text),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&self, message: String, line: usize, column: usize) -> LexError {
        LexError {
            message,
            line,
            column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_arrow_keywords_and_comments() {
        let tokens = lex("{ (x: i32) -> x + 1 } // hi\nif true { throw false } else {}").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Arrow));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Plus));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::If));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Else));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Throw));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Newline));
    }

    #[test]
    fn suppresses_newlines_in_parentheses_and_after_operators() {
        let tokens = lex(
            "f(\n  1,\n  2\n)\nlet x =\n  1 +\n  2\nlet y = x ??\n  3\nlet z = y ?.\n  value\n",
        )
        .unwrap();
        let newlines = tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Newline)
            .count();
        assert_eq!(newlines, 4);
        assert!(tokens
            .iter()
            .any(|token| token.kind == TokenKind::QuestionQuestion));
        assert!(tokens
            .iter()
            .any(|token| token.kind == TokenKind::QuestionDot));
    }

    #[test]
    fn recognizes_loops_and_suppresses_newlines_in_brackets() {
        let tokens = lex("while true { loop { break [\n  40,\n  2\n][\n0\n] } }\n").unwrap();
        assert!(tokens.iter().any(|token| token.kind == TokenKind::While));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Loop));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Break));
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.kind == TokenKind::LBracket)
                .count(),
            2
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Newline)
                .count(),
            1
        );
    }

    #[test]
    fn recognizes_extend_as_a_keyword() {
        let tokens = lex("extend A { let identity(T: type)(value: T) = value }").unwrap();
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Extend));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Type));
    }

    #[test]
    fn recognizes_trait_as_a_keyword() {
        let tokens = lex("let Foo = trait { let Item: type }").unwrap();
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Trait));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Type));
    }

    #[test]
    fn accepts_nested_block_comments() {
        let tokens = lex("let /* outer /* inner */ done */ x = 1").unwrap();
        assert!(tokens
            .iter()
            .any(|token| token.kind == TokenKind::Ident("x".into())));
    }

    #[test]
    fn reports_unterminated_block_comments() {
        let error = lex("let x = 1 /* no end").unwrap_err();
        assert_eq!((error.line, error.column), (1, 11));
        assert!(error.message.contains("unterminated"));
    }
}
