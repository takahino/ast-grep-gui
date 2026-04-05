use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use eframe::egui;

use crate::file_encoding::FileEncodingPreference;
use crate::i18n::UiLanguage;
use crate::search::{spawn_search, FileResult, SearchMessage, SearchMode};
use crate::sg_command::{is_sg_command, parse_sg_run};

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
            lock.push(TerminalLine { text, kind });
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

        let (tx, rx) = crossbeam_channel::unbounded();
        spawn_search(
            search_dir,
            args.pattern,
            args.lang,
            SearchMode::AstGrep,
            context_lines,
            String::new(),
            self.file_encoding_preference,
            10 * 1024 * 1024,
            0,
            ".git;target;node_modules".to_string(),
            UiLanguage::Japanese,
            crate::batch::SINGLE_SEARCH_JOB_ID,
            tx,
            egui_ctx.clone(),
        );

        let lines = Arc::clone(&self.lines);
        std::thread::spawn(move || {
            format_sg_results(rx, lines, egui_ctx);
        });
    }

    /// PowerShell にコマンドを委譲する
    fn run_powershell_command(&mut self, cmd: &str, egui_ctx: egui::Context) {
        let lines = Arc::clone(&self.lines);
        // 出力エンコーディングを UTF-8 に統一してから実行する（文字化け防止）
        let utf8_cmd = format!(
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
             [Console]::InputEncoding  = [Text.Encoding]::UTF8; \
             {}",
            cmd
        );
        let cwd = self.working_dir.clone();
        std::thread::spawn(move || {
            let result = std::process::Command::new("powershell.exe")
                .args(["-NonInteractive", "-NoProfile", "-Command", &utf8_cmd])
                .current_dir(&cwd)
                .output();

            let mut lock = match lines.lock() {
                Ok(l) => l,
                Err(_) => return,
            };
            match result {
                Ok(out) => {
                    let stdout = decode_output(&out.stdout);
                    for line in stdout.lines() {
                        lock.push(TerminalLine {
                            text: line.to_string(),
                            kind: LineKind::Stdout,
                        });
                    }
                    let stderr = decode_output(&out.stderr);
                    for line in stderr.lines() {
                        lock.push(TerminalLine {
                            text: line.to_string(),
                            kind: LineKind::Stderr,
                        });
                    }
                }
                Err(e) => {
                    lock.push(TerminalLine {
                        text: format!("コマンド実行エラー: {}", e),
                        kind: LineKind::Stderr,
                    });
                }
            }
            egui_ctx.request_repaint();
        });
    }
}

/// sg 検索結果を CLI 形式でターミナルに書き込む
fn format_sg_results(
    rx: Receiver<SearchMessage>,
    lines: Arc<Mutex<Vec<TerminalLine>>>,
    egui_ctx: egui::Context,
) {
    let mut file_count: usize = 0;
    let mut match_count: usize = 0;

    loop {
        match rx.recv() {
            Ok(SearchMessage::FileResult { file, .. }) => {
                file_count += 1;
                append_file_result(&lines, &file, &mut match_count);
                egui_ctx.request_repaint();
            }
            Ok(SearchMessage::Done { elapsed_ms, .. }) => {
                if let Ok(mut lock) = lines.lock() {
                    lock.push(TerminalLine {
                        text: format!(
                            "{} matches in {} files ({} ms)",
                            match_count, file_count, elapsed_ms
                        ),
                        kind: LineKind::Stdout,
                    });
                }
                egui_ctx.request_repaint();
                break;
            }
            Ok(SearchMessage::Error { msg, .. }) => {
                if let Ok(mut lock) = lines.lock() {
                    lock.push(TerminalLine {
                        text: format!("エラー: {}", msg),
                        kind: LineKind::Stderr,
                    });
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
) {
    let Ok(mut lock) = lines.lock() else { return };

    // ファイルパス行
    lock.push(TerminalLine {
        text: fr.relative_path.clone(),
        kind: LineKind::Stdout,
    });

    for m in &fr.matches {
        *match_count += 1;

        // コンテキスト前
        let ctx_before_start = m.line_start.saturating_sub(m.context_before.len());
        for (i, cl) in m.context_before.iter().enumerate() {
            lock.push(TerminalLine {
                text: format!("  {}│  {}", ctx_before_start + i, cl),
                kind: LineKind::Stdout,
            });
        }

        // マッチ行（複数行マッチ対応）
        for (i, ml) in m.span_lines_text.lines().enumerate() {
            let marker = if i == 0 { "◉" } else { " " };
            lock.push(TerminalLine {
                text: format!("  {}│{} {}", m.line_start + i, marker, ml),
                kind: LineKind::Stdout,
            });
        }

        // コンテキスト後
        for (i, cl) in m.context_after.iter().enumerate() {
            lock.push(TerminalLine {
                text: format!("  {}│  {}", m.line_end + 1 + i, cl),
                kind: LineKind::Stdout,
            });
        }
    }

    // ファイル間の区切り空行
    lock.push(TerminalLine {
        text: String::new(),
        kind: LineKind::Stdout,
    });
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

/// PowerShell 出力を UTF-8 → CP932 (Windows-31J) の順でデコードする
fn decode_output(bytes: &[u8]) -> String {
    // まず UTF-8 として解釈を試みる
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // UTF-8 でなければ Windows-31J (CP932 / Shift-JIS) としてデコード
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    decoded.into_owned()
}
