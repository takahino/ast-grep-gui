use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use eframe::egui;

use crate::file_encoding::{read_text_file_as, FileEncodingPreference};
use crate::i18n::UiLanguage;
use crate::search::{
    join_span_lines, search_message_channel, slice_context_lines, spawn_search, FileResult,
    MatchItem, PlainTextSearchOptions, SearchMessage, SearchMode, YamlRuleOptions,
    TERMINAL_MAX_LINES, TERMINAL_MAX_SEARCH_HITS,
};
use crate::sg_command::{is_sg_command, parse_sg_run};
use crate::type_hint_config::TypeHintConfig;

/// ターミナル行の種別（表示色の切り替えに使用）
#[derive(Debug, Clone)]
pub enum LineKind {
    /// プロンプト行（青色）
    Prompt,
    /// 標準出力（薄白）
    Stdout,
    /// 標準エラー（赤）
    Stderr,
}

/// ターミナルに表示する1行
#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub text: String,
    pub kind: LineKind,
}

/// ターミナルパネルの状態
pub struct TerminalState {
    /// 表示ライン（バックグラウンドスレッドから書き込まれるため Arc<Mutex>）
    pub lines: Arc<Mutex<Vec<TerminalLine>>>,
    /// 入力フィールドの現在テキスト
    pub input: String,
    /// コマンド入力履歴（新しい順）
    pub history: Vec<String>,
    /// ↑↓ キーでのナビゲーション位置
    pub history_idx: Option<usize>,
    /// 次回描画時に最下部へスクロールするフラグ
    pub scroll_to_bottom: bool,
    /// 現在の作業ディレクトリ
    pub working_dir: PathBuf,
    /// `sg` 実行時のファイル文字コード設定
    pub file_encoding_preference: FileEncodingPreference,
}

impl TerminalState {
    pub fn new(file_encoding_preference: FileEncodingPreference) -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
            input: String::new(),
            history: Vec::new(),
            history_idx: None,
            scroll_to_bottom: false,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_encoding_preference,
        }
    }

    /// プロンプト文字列を生成
    pub fn prompt_str(&self) -> String {
        format!("PS {}> ", self.working_dir.display())
    }

    /// コマンドをルーティングして実行する
    pub fn run_command(&mut self, cmd: &str, egui_ctx: egui::Context) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }

        // プロンプト行を追加
        let prompt = self.prompt_str();
        self.push_line(format!("{}{}", prompt, cmd), LineKind::Prompt);

        // 履歴に追加（重複排除、最大100件）
        self.history.retain(|h| h != cmd);
        self.history.insert(0, cmd.to_string());
        self.history.truncate(100);
        self.history_idx = None;

        self.scroll_to_bottom = true;

        if is_sg_command(cmd) {
            self.run_sg_command(cmd, egui_ctx);
        } else if let Some(new_dir) = parse_cd(cmd) {
            self.handle_cd(new_dir);
            egui_ctx.request_repaint();
        } else {
            self.run_powershell_command(cmd, egui_ctx);
        }
    }

    fn push_line(&self, text: String, kind: LineKind) {
        if let Ok(mut lock) = self.lines.lock() {
            push_line_with_limit(&mut lock, TerminalLine { text, kind });
        }
    }

    fn push_stderr(&self, text: String) {
        self.push_line(text, LineKind::Stderr);
    }

    /// `cd` コマンドをローカルで処理する
    fn handle_cd(&mut self, target: &str) {
        let new_dir = if target == "~" {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| self.working_dir.clone())
        } else {
            resolve_dir(&self.working_dir, target)
        };

        if new_dir.is_dir() {
            self.working_dir = new_dir;
        } else {
            self.push_stderr(format!("cd: ディレクトリが見つかりません: {}", target));
        }
    }

    /// sg コマンドを内蔵エンジンで実行する
    fn run_sg_command(&mut self, cmd: &str, egui_ctx: egui::Context) {
        let args = match parse_sg_run(cmd) {
            Ok(a) => a,
            Err(e) => {
                self.push_stderr(format!("sg: {}", e));
                egui_ctx.request_repaint();
                return;
            }
        };

        let search_dir = if args.search_dir.is_empty() {
            self.working_dir.to_string_lossy().to_string()
        } else {
            resolve_dir(&self.working_dir, &args.search_dir)
                .to_string_lossy()
                .to_string()
        };

        let context_lines = args.context_before.max(args.context_after);

        let (tx, rx) = search_message_channel();
        spawn_search(
            search_dir,
            args.pattern,
            args.lang,
            SearchMode::AstGrep,
            PlainTextSearchOptions::default(),
            YamlRuleOptions::default(),
            context_lines,
            String::new(),
            self.file_encoding_preference,
            10 * 1024 * 1024,
            TERMINAL_MAX_SEARCH_HITS,
            ".git;target;node_modules".to_string(),
            String::new(),
            true,
            Arc::new(TypeHintConfig::default()),
            UiLanguage::Japanese,
            crate::batch::SINGLE_SEARCH_JOB_ID,
            tx,
            Some(egui_ctx.clone()),
        );

        let lines = Arc::clone(&self.lines);
        std::thread::spawn(move || {
            format_sg_results(rx, lines, context_lines, egui_ctx);
        });
    }

    /// PowerShell にコマンドを委譲する（stdout/stderr をストリーミング読み取り）
    fn run_powershell_command(&mut self, cmd: &str, egui_ctx: egui::Context) {
        let lines = Arc::clone(&self.lines);
        let utf8_cmd = format!(
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
             [Console]::InputEncoding  = [Text.Encoding]::UTF8; \
             {}",
            cmd
        );
        let cwd = self.working_dir.clone();
        std::thread::spawn(move || {
            let mut child = match Command::new("powershell.exe")
                .args(["-NonInteractive", "-NoProfile", "-Command", &utf8_cmd])
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    if let Ok(mut lock) = lines.lock() {
                        push_line_with_limit(
                            &mut lock,
                            TerminalLine {
                                text: format!("コマンド実行エラー: {}", e),
                                kind: LineKind::Stderr,
                            },
                        );
                    }
                    egui_ctx.request_repaint();
                    return;
                }
            };

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            if let Some(out) = stdout {
                let lines_out = Arc::clone(&lines);
                let ctx = egui_ctx.clone();
                std::thread::spawn(move || {
                    stream_process_output(out, lines_out, LineKind::Stdout, ctx);
                });
            }
            if let Some(err) = stderr {
                let lines_err = Arc::clone(&lines);
                let ctx = egui_ctx.clone();
                std::thread::spawn(move || {
                    stream_process_output(err, lines_err, LineKind::Stderr, ctx);
                });
            }

            let _ = child.wait();
            egui_ctx.request_repaint();
        });
    }
}

fn push_line_with_limit(lines: &mut Vec<TerminalLine>, line: TerminalLine) {
    lines.push(line);
    if lines.len() > TERMINAL_MAX_LINES {
        let drop = lines.len() - TERMINAL_MAX_LINES;
        lines.drain(0..drop);
    }
}

fn stream_process_output<R: std::io::Read>(
    reader: R,
    lines: Arc<Mutex<Vec<TerminalLine>>>,
    kind: LineKind,
    egui_ctx: egui::Context,
) {
    let buffered = BufReader::new(reader);
    for line_result in buffered.lines() {
        let Ok(line) = line_result else {
            break;
        };
        if let Ok(mut lock) = lines.lock() {
            push_line_with_limit(
                &mut lock,
                TerminalLine {
                    text: line,
                    kind: kind.clone(),
                },
            );
        }
        egui_ctx.request_repaint();
    }
}

/// sg 検索結果を CLI 形式でターミナルに書き込む
fn format_sg_results(
    rx: Receiver<SearchMessage>,
    lines: Arc<Mutex<Vec<TerminalLine>>>,
    context_lines: usize,
    egui_ctx: egui::Context,
) {
    let mut file_count: usize = 0;
    let mut match_count: usize = 0;

    loop {
        match rx.recv() {
            Ok(SearchMessage::FileResult { file, .. }) => {
                file_count += 1;
                append_file_result(&lines, &file, &mut match_count, context_lines);
                egui_ctx.request_repaint();
            }
            Ok(SearchMessage::Done {
                elapsed_ms,
                hit_limit_reached,
                ..
            }) => {
                if let Ok(mut lock) = lines.lock() {
                    let mut summary = format!(
                        "{} matches in {} files ({} ms)",
                        match_count, file_count, elapsed_ms
                    );
                    if hit_limit_reached {
                        summary.push_str(&format!(" [hit limit: {TERMINAL_MAX_SEARCH_HITS}]"));
                    }
                    push_line_with_limit(
                        &mut lock,
                        TerminalLine {
                            text: summary,
                            kind: LineKind::Stdout,
                        },
                    );
                }
                egui_ctx.request_repaint();
                break;
            }
            Ok(SearchMessage::Error { msg, .. }) => {
                if let Ok(mut lock) = lines.lock() {
                    push_line_with_limit(
                        &mut lock,
                        TerminalLine {
                            text: format!("エラー: {}", msg),
                            kind: LineKind::Stderr,
                        },
                    );
                }
                egui_ctx.request_repaint();
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

/// 1ファイル分の結果を sg CLI 形式でターミナルラインに追記する
fn append_file_result(
    lines: &Arc<Mutex<Vec<TerminalLine>>>,
    fr: &FileResult,
    match_count: &mut usize,
    context_lines: usize,
) {
    let Ok(mut lock) = lines.lock() else {
        return;
    };

    push_line_with_limit(
        &mut lock,
        TerminalLine {
            text: fr.relative_path.clone(),
            kind: LineKind::Stdout,
        },
    );

    let source_lines: Option<Vec<String>> = read_text_file_as(&fr.path, fr.text_encoding.clone())
        .ok()
        .map(|s| s.lines().map(str::to_owned).collect());

    for m in &fr.matches {
        *match_count += 1;
        append_match_lines(&mut lock, m, context_lines, source_lines.as_deref());
    }

    push_line_with_limit(
        &mut lock,
        TerminalLine {
            text: String::new(),
            kind: LineKind::Stdout,
        },
    );
}

fn append_match_lines(
    lock: &mut Vec<TerminalLine>,
    m: &MatchItem,
    context_lines: usize,
    source_lines: Option<&[String]>,
) {
    if !m.context_before.is_empty() || !m.span_lines_text.is_empty() {
        let ctx_before_start = m.line_start.saturating_sub(m.context_before.len());
        for (i, cl) in m.context_before.iter().enumerate() {
            push_line_with_limit(
                lock,
                TerminalLine {
                    text: format!("  {}│  {}", ctx_before_start + i, cl),
                    kind: LineKind::Stdout,
                },
            );
        }
        for (i, ml) in m.span_lines_text.lines().enumerate() {
            let marker = if i == 0 { "◉" } else { " " };
            push_line_with_limit(
                lock,
                TerminalLine {
                    text: format!("  {}│{} {}", m.line_start + i, marker, ml),
                    kind: LineKind::Stdout,
                },
            );
        }
        for (i, cl) in m.context_after.iter().enumerate() {
            push_line_with_limit(
                lock,
                TerminalLine {
                    text: format!("  {}│  {}", m.line_end + 1 + i, cl),
                    kind: LineKind::Stdout,
                },
            );
        }
        return;
    }

    let Some(lines) = source_lines else {
        push_line_with_limit(
            lock,
            TerminalLine {
                text: format!("  {}│◉ {}", m.line_start, m.matched_text),
                kind: LineKind::Stdout,
            },
        );
        return;
    };

    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let ls0 = m.line_start.saturating_sub(1);
    let le0 = m.line_end.saturating_sub(1);
    let (context_before, context_after) = slice_context_lines(&line_refs, ls0, le0, context_lines);
    let span_text = join_span_lines(&line_refs, ls0, le0);

    let ctx_before_start = m.line_start.saturating_sub(context_before.len());
    for (i, cl) in context_before.iter().enumerate() {
        push_line_with_limit(
            lock,
            TerminalLine {
                text: format!("  {}│  {}", ctx_before_start + i, cl),
                kind: LineKind::Stdout,
            },
        );
    }
    for (i, ml) in span_text.lines().enumerate() {
        let marker = if i == 0 { "◉" } else { " " };
        push_line_with_limit(
            lock,
            TerminalLine {
                text: format!("  {}│{} {}", m.line_start + i, marker, ml),
                kind: LineKind::Stdout,
            },
        );
    }
    for (i, cl) in context_after.iter().enumerate() {
        push_line_with_limit(
            lock,
            TerminalLine {
                text: format!("  {}│  {}", m.line_end + 1 + i, cl),
                kind: LineKind::Stdout,
            },
        );
    }
}

/// `cd <target>` をパースして target 文字列を返す
fn parse_cd(cmd: &str) -> Option<&str> {
    let cmd = cmd.trim();
    if cmd == "cd" {
        return Some("~");
    }
    if let Some(rest) = cmd.strip_prefix("cd ") {
        let target = rest.trim().trim_matches('"').trim_matches('\'');
        if !target.is_empty() {
            return Some(target);
        }
    }
    None
}

/// 相対・絶対パスを解決して PathBuf を返す
fn resolve_dir(base: &Path, target: &str) -> PathBuf {
    let p = Path::new(target);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}
