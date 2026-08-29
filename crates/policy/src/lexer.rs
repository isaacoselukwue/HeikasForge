#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Css,
    Shell,
}

impl SourceLanguage {
    pub fn for_path(path: &str) -> Option<Self> {
        let extension = path.rsplit('.').next()?;
        match extension {
            "rs" => Some(SourceLanguage::Rust),
            "ts" | "tsx" => Some(SourceLanguage::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(SourceLanguage::JavaScript),
            "css" => Some(SourceLanguage::Css),
            "sh" | "bash" => Some(SourceLanguage::Shell),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SourceLanguage::Rust => "rust",
            SourceLanguage::TypeScript => "typescript",
            SourceLanguage::JavaScript => "javascript",
            SourceLanguage::Css => "css",
            SourceLanguage::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentSpan {
    pub line: u32,
    pub column: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanOutcome {
    pub comments: Vec<CommentSpan>,
    pub literals: Vec<CommentSpan>,
    pub code_only: String,
}

pub fn scan(language: SourceLanguage, source: &str) -> ScanOutcome {
    match language {
        SourceLanguage::Rust => scan_rust(source),
        SourceLanguage::TypeScript | SourceLanguage::JavaScript => scan_curly(source),
        SourceLanguage::Css => scan_css(source),
        SourceLanguage::Shell => scan_shell(source),
    }
}

struct Cursor<'a> {
    characters: Vec<char>,
    index: usize,
    line: u32,
    column: u32,
    comments: Vec<CommentSpan>,
    literals: Vec<CommentSpan>,
    code_only: String,
    source: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            characters: source.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
            comments: Vec::new(),
            literals: Vec::new(),
            code_only: String::new(),
            source,
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.characters.get(self.index + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.characters.get(self.index).copied()?;
        self.index += 1;
        if character == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(character)
    }

    fn emit(&mut self, character: char) {
        self.code_only.push(character);
    }

    fn finish(self) -> ScanOutcome {
        let _ = self.source;
        ScanOutcome {
            comments: self.comments,
            literals: self.literals,
            code_only: self.code_only,
        }
    }

    fn record_comment(&mut self, line: u32, column: u32, text: String) {
        self.comments.push(CommentSpan { line, column, text });
    }

    fn consume_line_comment(&mut self, marker_length: usize) {
        let line = self.line;
        let column = self.column;
        for _ in 0..marker_length {
            self.advance();
        }
        let mut text = String::new();
        while let Some(character) = self.peek(0) {
            if character == '\n' {
                break;
            }
            text.push(character);
            self.advance();
        }
        self.record_comment(line, column, text.trim().to_string());
    }

    fn consume_block_comment(&mut self, allow_nesting: bool) {
        let line = self.line;
        let column = self.column;
        self.advance();
        self.advance();
        let mut depth = 1usize;
        let mut text = String::new();
        while let Some(character) = self.peek(0) {
            if allow_nesting && character == '/' && self.peek(1) == Some('*') {
                depth += 1;
                text.push_str("/*");
                self.advance();
                self.advance();
                continue;
            }
            if character == '*' && self.peek(1) == Some('/') {
                depth -= 1;
                self.advance();
                self.advance();
                if depth == 0 {
                    break;
                }
                text.push_str("*/");
                continue;
            }
            text.push(character);
            if character == '\n' {
                self.code_only.push('\n');
            }
            self.advance();
        }
        self.record_comment(line, column, text.trim().to_string());
    }

    fn consume_string(&mut self, terminator: char, allow_escapes: bool) {
        let line = self.line;
        let column = self.column;
        let mut content = String::new();
        let opener = self.advance();
        if let Some(opener) = opener {
            self.emit(opener);
        }
        while let Some(character) = self.peek(0) {
            if allow_escapes && character == '\\' {
                self.advance();
                if let Some(escaped) = self.advance() {
                    self.emit(escaped);
                    content.push(escaped);
                }
                continue;
            }
            self.advance();
            self.emit(character);
            if character == terminator {
                break;
            }
            content.push(character);
        }
        self.literals.push(CommentSpan {
            line,
            column,
            text: content,
        });
    }
}

fn scan_rust(source: &str) -> ScanOutcome {
    let mut cursor = Cursor::new(source);
    while let Some(character) = cursor.peek(0) {
        match character {
            '/' if cursor.peek(1) == Some('/') => cursor.consume_line_comment(2),
            '/' if cursor.peek(1) == Some('*') => cursor.consume_block_comment(true),
            '"' => cursor.consume_string('"', true),
            'r' if matches!(cursor.peek(1), Some('"') | Some('#')) => {
                consume_rust_raw_string(&mut cursor);
            }
            'b' if cursor.peek(1) == Some('"') => {
                cursor.advance();
                cursor.emit('b');
                cursor.consume_string('"', true);
            }
            '\'' => consume_rust_quote(&mut cursor),
            other => {
                cursor.advance();
                cursor.emit(other);
            }
        }
    }
    cursor.finish()
}

fn consume_rust_raw_string(cursor: &mut Cursor<'_>) {
    let mut hashes = 0usize;
    let mut lookahead = 1usize;
    while cursor.peek(lookahead) == Some('#') {
        hashes += 1;
        lookahead += 1;
    }
    if cursor.peek(lookahead) != Some('"') {
        if let Some(character) = cursor.advance() {
            cursor.emit(character);
        }
        return;
    }
    for _ in 0..=lookahead {
        if let Some(character) = cursor.advance() {
            cursor.emit(character);
        }
    }
    loop {
        let Some(character) = cursor.peek(0) else {
            break;
        };
        if character == '"' {
            let mut matched = 0usize;
            while cursor.peek(1 + matched) == Some('#') && matched < hashes {
                matched += 1;
            }
            if matched == hashes {
                for _ in 0..=hashes {
                    if let Some(consumed) = cursor.advance() {
                        cursor.emit(consumed);
                    }
                }
                break;
            }
        }
        cursor.advance();
        cursor.emit(character);
    }
}

fn consume_rust_quote(cursor: &mut Cursor<'_>) {
    let next = cursor.peek(1);
    let after = cursor.peek(2);
    let is_lifetime = matches!(next, Some(character) if character.is_alphabetic() || character == '_')
        && after != Some('\'');
    if is_lifetime {
        if let Some(character) = cursor.advance() {
            cursor.emit(character);
        }
        return;
    }
    cursor.consume_string('\'', true);
}

fn scan_curly(source: &str) -> ScanOutcome {
    let mut cursor = Cursor::new(source);
    let mut previous_significant: Option<char> = None;
    while let Some(character) = cursor.peek(0) {
        match character {
            '/' if cursor.peek(1) == Some('/') => {
                cursor.consume_line_comment(2);
                previous_significant = None;
            }
            '/' if cursor.peek(1) == Some('*') => {
                cursor.consume_block_comment(false);
                previous_significant = None;
            }
            '/' if regex_literal_allowed(previous_significant) => {
                consume_regex_literal(&mut cursor);
                previous_significant = Some('/');
            }
            '"' => {
                cursor.consume_string('"', true);
                previous_significant = Some('"');
            }
            '\'' => {
                cursor.consume_string('\'', true);
                previous_significant = Some('\'');
            }
            '`' => {
                consume_template_literal(&mut cursor);
                previous_significant = Some('`');
            }
            other => {
                cursor.advance();
                cursor.emit(other);
                if !other.is_whitespace() {
                    previous_significant = Some(other);
                }
            }
        }
    }
    cursor.finish()
}

fn regex_literal_allowed(previous: Option<char>) -> bool {
    match previous {
        None => true,
        Some(character) => matches!(
            character,
            '(' | ',' | '=' | ':' | '[' | '!' | '&' | '|' | '?' | '{' | '}' | ';' | '+' | '-' | '*' | '%' | '<' | '>' | '~' | '^'
        ),
    }
}

fn consume_regex_literal(cursor: &mut Cursor<'_>) {
    if let Some(character) = cursor.advance() {
        cursor.emit(character);
    }
    let mut in_class = false;
    while let Some(character) = cursor.peek(0) {
        match character {
            '\\' => {
                cursor.advance();
                cursor.emit('\\');
                if let Some(escaped) = cursor.advance() {
                    cursor.emit(escaped);
                }
            }
            '[' => {
                in_class = true;
                cursor.advance();
                cursor.emit('[');
            }
            ']' => {
                in_class = false;
                cursor.advance();
                cursor.emit(']');
            }
            '/' if !in_class => {
                cursor.advance();
                cursor.emit('/');
                break;
            }
            '\n' => break,
            other => {
                cursor.advance();
                cursor.emit(other);
            }
        }
    }
}

fn consume_template_literal(cursor: &mut Cursor<'_>) {
    let line = cursor.line;
    let column = cursor.column;
    let mut content = String::new();
    if let Some(character) = cursor.advance() {
        cursor.emit(character);
    }
    let mut depth = 0usize;
    while let Some(character) = cursor.peek(0) {
        match character {
            '\\' => {
                cursor.advance();
                cursor.emit('\\');
                if let Some(escaped) = cursor.advance() {
                    cursor.emit(escaped);
                }
            }
            '$' if cursor.peek(1) == Some('{') => {
                depth += 1;
                cursor.advance();
                cursor.advance();
                cursor.emit('$');
                cursor.emit('{');
            }
            '}' if depth > 0 => {
                depth -= 1;
                cursor.advance();
                cursor.emit('}');
            }
            '`' if depth == 0 => {
                cursor.advance();
                cursor.emit('`');
                break;
            }
            other => {
                cursor.advance();
                cursor.emit(other);
                if depth == 0 {
                    content.push(other);
                }
            }
        }
    }
    cursor.literals.push(CommentSpan {
        line,
        column,
        text: content,
    });
}

fn scan_css(source: &str) -> ScanOutcome {
    let mut cursor = Cursor::new(source);
    while let Some(character) = cursor.peek(0) {
        match character {
            '/' if cursor.peek(1) == Some('*') => cursor.consume_block_comment(false),
            '"' => cursor.consume_string('"', true),
            '\'' => cursor.consume_string('\'', true),
            other => {
                cursor.advance();
                cursor.emit(other);
            }
        }
    }
    cursor.finish()
}

fn scan_shell(source: &str) -> ScanOutcome {
    let mut cursor = Cursor::new(source);
    let mut at_line_start = true;
    while let Some(character) = cursor.peek(0) {
        match character {
            '#' if cursor.line == 1 && cursor.column == 1 && cursor.peek(1) == Some('!') => {
                while let Some(consumed) = cursor.peek(0) {
                    if consumed == '\n' {
                        break;
                    }
                    cursor.advance();
                    cursor.emit(consumed);
                }
            }
            '#' if at_line_start || preceded_by_whitespace(&cursor) => {
                cursor.consume_line_comment(1);
                at_line_start = false;
            }
            '"' => {
                cursor.consume_string('"', true);
                at_line_start = false;
            }
            '\'' => {
                cursor.consume_string('\'', false);
                at_line_start = false;
            }
            other => {
                cursor.advance();
                cursor.emit(other);
                at_line_start = other == '\n';
            }
        }
    }
    cursor.finish()
}

fn preceded_by_whitespace(cursor: &Cursor<'_>) -> bool {
    match cursor.code_only.chars().next_back() {
        None => true,
        Some(character) => character.is_whitespace(),
    }
}
