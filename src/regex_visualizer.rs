use regex::Regex;

use crate::i18n::UiLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexVisualKind {
    Anchor,
    Group,
    GroupEnd,
    Alternation,
    Quantifier,
    CharClass,
    Escape,
    Wildcard,
    Literal,
    Error,
}

#[derive(Debug, Clone)]
pub struct RegexVisualLine {
    pub depth: usize,
    pub kind: RegexVisualKind,
    pub token: String,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct RegexVisualStats {
    pub groups: usize,
    pub alternations: usize,
    pub char_classes: usize,
    pub quantifiers: usize,
}

#[derive(Debug, Clone)]
pub struct RegexVisualization {
    pub is_valid: bool,
    pub compile_error: Option<String>,
    pub stats: RegexVisualStats,
    pub lines: Vec<RegexVisualLine>,
    pub diagram: RegexDiagram,
}

#[derive(Debug, Clone)]
pub struct RegexDiagram {
    pub root: RegexDiagramNode,
    pub note: String,
}

/// グループ枠のラベルとホバー説明（UI 言語に合わせた文字列）
#[derive(Debug, Clone)]
pub struct RegexGroupMeta {
    pub title: String,
    pub tooltip: String,
}

#[derive(Debug, Clone)]
pub enum RegexDiagramNode {
    Empty,
    Token {
        label: String,
        kind: RegexVisualKind,
    },
    Sequence(Vec<RegexDiagramNode>),
    Alternation(Vec<RegexDiagramNode>),
    Repeat {
        child: Box<RegexDiagramNode>,
        quantifier: String,
    },
    Group {
        child: Box<RegexDiagramNode>,
        meta: RegexGroupMeta,
    },
}

pub fn visualize_regex(pattern: &str, lang: UiLanguage) -> RegexVisualization {
    let compile_error = Regex::new(pattern).err().map(|e| e.to_string());
    let is_valid = compile_error.is_none();
    let mut parser = Parser::new(pattern, lang);
    let lines = parser.parse();
    let diagram = build_diagram(pattern, lang);

    RegexVisualization {
        is_valid,
        compile_error,
        stats: parser.stats,
        lines,
        diagram,
    }
}

struct Parser {
    chars: Vec<char>,
    index: usize,
    depth: usize,
    lang: UiLanguage,
    stats: RegexVisualStats,
}

impl Parser {
    fn new(pattern: &str, lang: UiLanguage) -> Self {
        Self {
            chars: pattern.chars().collect(),
            index: 0,
            depth: 0,
            lang,
            stats: RegexVisualStats::default(),
        }
    }

    fn parse(&mut self) -> Vec<RegexVisualLine> {
        let mut lines = Vec::new();

        while self.index < self.chars.len() {
            match self.chars[self.index] {
                '\\' => lines.push(self.parse_escape()),
                '[' => lines.push(self.parse_char_class()),
                '(' => lines.push(self.parse_group_start()),
                ')' => lines.push(self.parse_group_end()),
                '|' => {
                    self.stats.alternations += 1;
                    lines.push(self.simple_line(
                        RegexVisualKind::Alternation,
                        "|".to_string(),
                        tr(self.lang, "分岐", "Alternation"),
                        tr(
                            self.lang,
                            "左右どちらかにマッチします",
                            "Matches either the left or right branch",
                        ),
                    ));
                    self.index += 1;
                }
                '^' | '$' => {
                    let ch = self.chars[self.index];
                    let note = if ch == '^' {
                        tr(self.lang, "行頭または文字列先頭", "Start of line or text")
                    } else {
                        tr(self.lang, "行末または文字列末尾", "End of line or text")
                    };
                    lines.push(self.simple_line(
                        RegexVisualKind::Anchor,
                        ch.to_string(),
                        tr(self.lang, "アンカー", "Anchor"),
                        note,
                    ));
                    self.index += 1;
                }
                '.' => {
                    lines.push(self.simple_line(
                        RegexVisualKind::Wildcard,
                        ".".to_string(),
                        tr(self.lang, "任意 1 文字", "Any character"),
                        tr(
                            self.lang,
                            "改行以外の任意の 1 文字にマッチします",
                            "Matches any single character except newline",
                        ),
                    ));
                    self.index += 1;
                }
                '*' | '+' | '?' => lines.push(self.parse_simple_quantifier()),
                '{' => {
                    if let Some(line) = self.parse_braced_quantifier() {
                        lines.push(line);
                    } else {
                        lines.push(self.parse_literal());
                    }
                }
                _ => lines.push(self.parse_literal()),
            }
        }

        if self.depth > 0 {
            lines.push(RegexVisualLine {
                depth: self.depth.saturating_sub(1),
                kind: RegexVisualKind::Error,
                token: tr(self.lang, "(未閉じグループ)", "(unclosed group)").to_string(),
                label: tr(self.lang, "注意", "Warning").to_string(),
                note: tr(
                    self.lang,
                    "閉じていない `)` があるようです",
                    "A closing `)` appears to be missing",
                )
                .to_string(),
            });
        }

        lines
    }

    fn parse_escape(&mut self) -> RegexVisualLine {
        let start = self.index;
        self.index += 1;
        if self.index >= self.chars.len() {
            return RegexVisualLine {
                depth: self.depth,
                kind: RegexVisualKind::Error,
                token: "\\".to_string(),
                label: tr(self.lang, "不正なエスケープ", "Invalid escape").to_string(),
                note: tr(
                    self.lang,
                    "末尾の `\\` は解釈できません",
                    "A trailing `\\` cannot be parsed",
                )
                .to_string(),
            };
        }

        self.index += 1;
        let token = self.chars[start..self.index].iter().collect::<String>();
        let note = match token.as_str() {
            r"\d" => tr(self.lang, "数字 1 文字", "A single digit"),
            r"\D" => tr(self.lang, "数字以外 1 文字", "A single non-digit"),
            r"\w" => tr(self.lang, "単語文字 1 文字", "A single word character"),
            r"\W" => tr(self.lang, "単語文字以外 1 文字", "A single non-word character"),
            r"\s" => tr(self.lang, "空白文字 1 文字", "A single whitespace character"),
            r"\S" => tr(
                self.lang,
                "空白以外 1 文字",
                "A single non-whitespace character",
            ),
            r"\b" => tr(self.lang, "単語境界", "Word boundary"),
            r"\B" => tr(self.lang, "単語境界ではない位置", "Not a word boundary"),
            r"\n" => tr(self.lang, "改行", "Newline"),
            r"\r" => tr(self.lang, "復帰", "Carriage return"),
            r"\t" => tr(self.lang, "タブ", "Tab"),
            _ => tr(
                self.lang,
                "次の文字を特別扱いせずそのまま解釈します",
                "Treats the next character literally or as a regex escape",
            ),
        };

        self.simple_line(
            RegexVisualKind::Escape,
            token,
            tr(self.lang, "エスケープ", "Escape"),
            note,
        )
    }

    fn parse_char_class(&mut self) -> RegexVisualLine {
        let start = self.index;
        self.index += 1;
        let mut escaped = false;
        let mut closed = false;

        while self.index < self.chars.len() {
            let ch = self.chars[self.index];
            self.index += 1;
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == ']' {
                closed = true;
                break;
            }
        }

        self.stats.char_classes += 1;
        let token = self.chars[start..self.index].iter().collect::<String>();
        let note = if closed {
            tr(
                self.lang,
                "この中のいずれか 1 文字にマッチします",
                "Matches one character from this character class",
            )
        } else {
            tr(
                self.lang,
                "文字クラスが `]` で閉じられていません",
                "The character class is missing a closing `]`",
            )
        };

        self.simple_line(
            if closed {
                RegexVisualKind::CharClass
            } else {
                RegexVisualKind::Error
            },
            token,
            tr(self.lang, "文字クラス", "Character class"),
            note,
        )
    }

    fn parse_group_start(&mut self) -> RegexVisualLine {
        let start_depth = self.depth;
        self.depth += 1;
        self.stats.groups += 1;

        let (token, note) = if self.peek_str("(?:" ) {
            self.index += 3;
            (
                "(?:".to_string(),
                tr(
                    self.lang,
                    "非キャプチャグループ。まとまりとして扱いますが、キャプチャはしません",
                    "Non-capturing group. Groups terms together without capturing",
                ),
            )
        } else if self.peek_str("(?=") {
            self.index += 3;
            (
                "(?=".to_string(),
                tr(
                    self.lang,
                    "先読み。Rust の regex では未対応です",
                    "Lookahead. Not supported by Rust regex",
                ),
            )
        } else if self.peek_str("(?!") {
            self.index += 3;
            (
                "(?!".to_string(),
                tr(
                    self.lang,
                    "否定先読み。Rust の regex では未対応です",
                    "Negative lookahead. Not supported by Rust regex",
                ),
            )
        } else if self.peek_str("(?<=") {
            self.index += 4;
            (
                "(?<=".to_string(),
                tr(
                    self.lang,
                    "後読み。Rust の regex では未対応です",
                    "Lookbehind. Not supported by Rust regex",
                ),
            )
        } else if self.peek_str("(?<!") {
            self.index += 4;
            (
                "(?<!".to_string(),
                tr(
                    self.lang,
                    "否定後読み。Rust の regex では未対応です",
                    "Negative lookbehind. Not supported by Rust regex",
                ),
            )
        } else {
            self.index += 1;
            (
                "(".to_string(),
                tr(
                    self.lang,
                    "キャプチャグループ。後でまとまりを参照しやすくします",
                    "Capturing group. Wraps a sub-pattern as a single unit",
                ),
            )
        };

        RegexVisualLine {
            depth: start_depth,
            kind: RegexVisualKind::Group,
            token,
            label: tr(self.lang, "グループ開始", "Group start").to_string(),
            note: note.to_string(),
        }
    }

    fn parse_group_end(&mut self) -> RegexVisualLine {
        self.depth = self.depth.saturating_sub(1);
        self.index += 1;

        RegexVisualLine {
            depth: self.depth,
            kind: RegexVisualKind::GroupEnd,
            token: ")".to_string(),
            label: tr(self.lang, "グループ終了", "Group end").to_string(),
            note: tr(
                self.lang,
                "ここでグループのまとまりが閉じます",
                "Closes the current group",
            )
            .to_string(),
        }
    }

    fn parse_simple_quantifier(&mut self) -> RegexVisualLine {
        self.stats.quantifiers += 1;
        let ch = self.chars[self.index];
        self.index += 1;
        let note = match ch {
            '*' => tr(self.lang, "直前要素の 0 回以上", "Zero or more of the previous item"),
            '+' => tr(self.lang, "直前要素の 1 回以上", "One or more of the previous item"),
            '?' => tr(self.lang, "直前要素の 0 回または 1 回", "Zero or one of the previous item"),
            _ => "",
        };

        self.simple_line(
            RegexVisualKind::Quantifier,
            ch.to_string(),
            tr(self.lang, "量指定", "Quantifier"),
            note,
        )
    }

    fn parse_braced_quantifier(&mut self) -> Option<RegexVisualLine> {
        let start = self.index;
        let mut cursor = self.index + 1;
        let mut saw_digit = false;

        while cursor < self.chars.len() {
            let ch = self.chars[cursor];
            if ch.is_ascii_digit() {
                saw_digit = true;
                cursor += 1;
                continue;
            }
            if ch == ',' {
                cursor += 1;
                continue;
            }
            if ch == '}' {
                cursor += 1;
                if !saw_digit {
                    return None;
                }
                self.index = cursor;
                self.stats.quantifiers += 1;
                let token = self.chars[start..cursor].iter().collect::<String>();
                return Some(self.simple_line(
                    RegexVisualKind::Quantifier,
                    token,
                    tr(self.lang, "量指定", "Quantifier"),
                    tr(
                        self.lang,
                        "直前要素の回数範囲を指定します",
                        "Sets an explicit repetition range for the previous item",
                    ),
                ));
            }
            return None;
        }

        None
    }

    fn parse_literal(&mut self) -> RegexVisualLine {
        let start = self.index;
        while self.index < self.chars.len() && !is_meta(self.chars[self.index]) {
            self.index += 1;
        }

        if start == self.index {
            self.index += 1;
        }

        let token = self.chars[start..self.index].iter().collect::<String>();
        self.simple_line(
            RegexVisualKind::Literal,
            token,
            tr(self.lang, "文字列", "Literal"),
            tr(
                self.lang,
                "この文字列そのものにマッチします",
                "Matches this text literally",
            ),
        )
    }

    fn simple_line(
        &self,
        kind: RegexVisualKind,
        token: String,
        label: &str,
        note: &str,
    ) -> RegexVisualLine {
        RegexVisualLine {
            depth: self.depth,
            kind,
            token,
            label: label.to_string(),
            note: note.to_string(),
        }
    }

    fn peek_str(&self, s: &str) -> bool {
        let needle: Vec<char> = s.chars().collect();
        self.chars[self.index..].starts_with(&needle)
    }
}

fn is_meta(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '[' | ']' | '(' | ')' | '|' | '^' | '$' | '.' | '*' | '+' | '?' | '{'
    )
}

fn tr<'a>(lang: UiLanguage, ja: &'a str, en: &'a str) -> &'a str {
    match lang {
        UiLanguage::Japanese => ja,
        UiLanguage::English => en,
    }
}

fn build_diagram(pattern: &str, lang: UiLanguage) -> RegexDiagram {
    let mut parser = DiagramParser::new(pattern, lang);
    let root = parser.parse_expression();
    RegexDiagram {
        root,
        note: tr(
            lang,
            "Regulex 風に、`|` の分岐を上下レーンに分けて表示します。`* + ?` や `{m,n}` は迂回線やループ線で表します。グループは枠で囲み、ホバーで説明を表示します。",
            "A Regulex-like view that splits `|` into separate lanes. `* + ?` and `{m,n}` use bypass and loop paths. Groups are framed; hover for details.",
        )
        .to_string(),
    }
}

struct DiagramParser {
    chars: Vec<char>,
    index: usize,
    lang: UiLanguage,
}

impl DiagramParser {
    fn new(pattern: &str, lang: UiLanguage) -> Self {
        Self {
            chars: pattern.chars().collect(),
            index: 0,
            lang,
        }
    }

    fn tr(&self, ja: &'static str, en: &'static str) -> &'static str {
        tr(self.lang, ja, en)
    }

    fn parse_expression(&mut self) -> RegexDiagramNode {
        let mut branches = vec![self.parse_sequence()];
        while self.peek('|') {
            self.index += 1;
            branches.push(self.parse_sequence());
        }

        if branches.len() == 1 {
            branches.pop().unwrap_or(RegexDiagramNode::Empty)
        } else {
            RegexDiagramNode::Alternation(branches)
        }
    }

    fn parse_sequence(&mut self) -> RegexDiagramNode {
        let mut items = Vec::new();
        while self.index < self.chars.len() {
            let ch = self.chars[self.index];
            if ch == ')' || ch == '|' {
                break;
            }
            items.push(self.parse_postfix());
        }

        match items.len() {
            0 => RegexDiagramNode::Empty,
            1 => items.pop().unwrap_or(RegexDiagramNode::Empty),
            _ => RegexDiagramNode::Sequence(items),
        }
    }

    fn parse_postfix(&mut self) -> RegexDiagramNode {
        let atom = self.parse_atom();
        if self.index >= self.chars.len() {
            return atom;
        }

        match self.chars[self.index] {
            '*' | '+' | '?' => {
                let quantifier = self.chars[self.index].to_string();
                self.index += 1;
                RegexDiagramNode::Repeat {
                    child: Box::new(atom),
                    quantifier,
                }
            }
            '{' => {
                if let Some(quantifier) = self.read_braced_quantifier() {
                    RegexDiagramNode::Repeat {
                        child: Box::new(atom),
                        quantifier,
                    }
                } else {
                    atom
                }
            }
            _ => atom,
        }
    }

    fn parse_atom(&mut self) -> RegexDiagramNode {
        if self.index >= self.chars.len() {
            return RegexDiagramNode::Empty;
        }

        match self.chars[self.index] {
            '(' => self.parse_group(),
            '[' => RegexDiagramNode::Token {
                label: self.read_char_class(),
                kind: RegexVisualKind::CharClass,
            },
            '\\' => RegexDiagramNode::Token {
                label: self.read_escape(),
                kind: RegexVisualKind::Escape,
            },
            '.' => {
                self.index += 1;
                RegexDiagramNode::Token {
                    label: ".".to_string(),
                    kind: RegexVisualKind::Wildcard,
                }
            }
            '^' | '$' => {
                let ch = self.chars[self.index];
                self.index += 1;
                RegexDiagramNode::Token {
                    label: ch.to_string(),
                    kind: RegexVisualKind::Anchor,
                }
            }
            _ => RegexDiagramNode::Token {
                label: self.read_literal(),
                kind: RegexVisualKind::Literal,
            },
        }
    }

    fn parse_group(&mut self) -> RegexDiagramNode {
        debug_assert_eq!(self.chars.get(self.index), Some(&'('));
        self.index += 1;

        let meta = if self.peek_str("?:") {
            self.index += 2;
            RegexGroupMeta {
                title: self.tr("非キャプチャ", "Non-capturing").to_string(),
                tooltip: self
                    .tr(
                        "(?: … ) はマッチのまとまりですが番号付きキャプチャにはしません。",
                        "(?: … ) groups the pattern without creating a numbered capture.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?=") {
            self.index += 2;
            RegexGroupMeta {
                title: self.tr("先読み", "Lookahead").to_string(),
                tooltip: self
                    .tr(
                        "(?= … ) は先読み（このアプリの検索は Rust regex で、先読みは未対応です）。",
                        "(?= … ) is lookahead. Rust `regex` does not support lookaround assertions.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?!") {
            self.index += 2;
            RegexGroupMeta {
                title: self.tr("否定先読み", "Neg. lookahead").to_string(),
                tooltip: self
                    .tr(
                        "(?! … ) は否定先読み（Rust regex では未対応）。",
                        "(?! … ) is negative lookahead. Not supported by Rust `regex`.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?<=") {
            self.index += 3;
            RegexGroupMeta {
                title: self.tr("後読み", "Lookbehind").to_string(),
                tooltip: self
                    .tr(
                        "(?<= … ) は後読み（Rust regex では未対応）。",
                        "(?<= … ) is lookbehind. Not supported by Rust `regex`.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?<!") {
            self.index += 3;
            RegexGroupMeta {
                title: self.tr("否定後読み", "Neg. lookbehind").to_string(),
                tooltip: self
                    .tr(
                        "(?<! … ) は否定後読み（Rust regex では未対応）。",
                        "(?<! … ) is negative lookbehind. Not supported by Rust `regex`.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?P<") {
            self.skip_named_group_after_prefix(3);
            RegexGroupMeta {
                title: self.tr("名前付きキャプチャ", "Named capture").to_string(),
                tooltip: self
                    .tr(
                        "(?P<name> … ) は名前付きグループ。後方参照や置換で名前で参照できます。",
                        "(?P<name> … ) is a named capture group for backreferences and replacements.",
                    )
                    .to_string(),
            }
        } else if self.peek_str("?<") {
            self.skip_named_group_after_prefix(2);
            RegexGroupMeta {
                title: self.tr("名前付きキャプチャ", "Named capture").to_string(),
                tooltip: self
                    .tr(
                        "(?<name> … ) は名前付きグループ（Rust regex で利用可能）。",
                        "(?<name> … ) is a named capture group (supported by Rust `regex`).",
                    )
                    .to_string(),
            }
        } else {
            RegexGroupMeta {
                title: self.tr("キャプチャ", "Capturing").to_string(),
                tooltip: self
                    .tr(
                        "( … ) は番号付きキャプチャグループ。同じパターン内で \\1 などで参照できます。",
                        "( … ) is a numbered capture group; refer with \\1, \\2, … in the same pattern.",
                    )
                    .to_string(),
            }
        };

        let inner = self.parse_expression();
        if self.peek(')') {
            self.index += 1;
        }

        RegexDiagramNode::Group {
            child: Box::new(inner),
            meta,
        }
    }

    fn skip_named_group_after_prefix(&mut self, prefix_len: usize) {
        self.index += prefix_len;
        while self.index < self.chars.len() && self.chars[self.index] != '>' {
            self.index += 1;
        }
        if self.peek('>') {
            self.index += 1;
        }
    }

    fn read_escape(&mut self) -> String {
        let start = self.index;
        self.index += 1;
        if self.index < self.chars.len() {
            self.index += 1;
        }
        self.chars[start..self.index].iter().collect()
    }

    fn read_char_class(&mut self) -> String {
        let start = self.index;
        self.index += 1;
        let mut escaped = false;
        while self.index < self.chars.len() {
            let ch = self.chars[self.index];
            self.index += 1;
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == ']' {
                break;
            }
        }
        self.chars[start..self.index].iter().collect()
    }

    fn read_literal(&mut self) -> String {
        let start = self.index;
        while self.index < self.chars.len() {
            let ch = self.chars[self.index];
            if matches!(
                ch,
                '(' | ')' | '[' | ']' | '\\' | '|' | '*' | '+' | '?' | '{' | '}' | '.' | '^' | '$'
            ) {
                break;
            }
            self.index += 1;
        }
        if start == self.index {
            self.index += 1;
        }
        self.chars[start..self.index].iter().collect()
    }

    fn read_braced_quantifier(&mut self) -> Option<String> {
        let start = self.index;
        let mut cursor = self.index + 1;
        let mut saw_digit = false;
        while cursor < self.chars.len() {
            let ch = self.chars[cursor];
            if ch.is_ascii_digit() {
                saw_digit = true;
                cursor += 1;
                continue;
            }
            if ch == ',' {
                cursor += 1;
                continue;
            }
            if ch == '}' {
                if !saw_digit {
                    return None;
                }
                cursor += 1;
                self.index = cursor;
                return Some(self.chars[start..cursor].iter().collect());
            }
            return None;
        }
        None
    }

    fn peek(&self, ch: char) -> bool {
        self.chars.get(self.index).copied() == Some(ch)
    }

    fn peek_str(&self, s: &str) -> bool {
        let needle: Vec<char> = s.chars().collect();
        self.chars[self.index..].starts_with(&needle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visualize_nested_regex() {
        let vis = visualize_regex(r"^(foo|bar)+\d{2}$", UiLanguage::English);
        assert!(vis.is_valid);
        assert_eq!(vis.stats.groups, 1);
        assert_eq!(vis.stats.alternations, 1);
        assert_eq!(vis.stats.quantifiers, 2);
        assert!(vis.lines.iter().any(|line| line.token == r"\d"));
    }

    #[test]
    fn visualize_character_class() {
        let vis = visualize_regex(r"[A-Z_][A-Za-z0-9_]*", UiLanguage::Japanese);
        assert!(vis.is_valid);
        assert_eq!(vis.stats.char_classes, 2);
        assert!(vis.lines.iter().any(|line| line.label.contains("文字クラス")));
    }

    #[test]
    fn diagram_builds_alternation_tree() {
        let vis = visualize_regex(r"(a|b).+$", UiLanguage::Japanese);
        let has_alt = match &vis.diagram.root {
            RegexDiagramNode::Sequence(items) => items.iter().any(|item| {
                matches!(
                    item,
                    RegexDiagramNode::Alternation(_) | RegexDiagramNode::Group { .. }
                )
            }),
            RegexDiagramNode::Alternation(_) => true,
            RegexDiagramNode::Group { .. } => true,
            _ => false,
        };
        assert!(has_alt);
    }

    #[test]
    fn diagram_wraps_paren_in_group() {
        let vis = visualize_regex(r"(a|b)", UiLanguage::Japanese);
        match &vis.diagram.root {
            RegexDiagramNode::Group { child, meta } => {
                assert!(meta.title.contains("キャプチャ") || meta.title.contains("Capturing"));
                assert!(matches!(child.as_ref(), RegexDiagramNode::Alternation(_)));
            }
            _ => panic!("expected Group around (a|b)"),
        }
    }
}
