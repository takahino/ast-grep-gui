use crate::lang::SupportedLanguage;

/// `sg run` コマンドのパース結果
pub struct SgRunArgs {
    pub pattern: String,
    pub lang: SupportedLanguage,
    pub context_before: usize,
    pub context_after: usize,
    /// 未指定なら空文字列（呼び出し側で CWD を使う）
    pub search_dir: String,
}

/// "sg run -p PAT [-l LANG] [-C N] [-A N] [-B N] [DIR]" をパース。
/// "sg -p PAT ..."、"-p PAT ..." も受け付ける。
pub fn parse_sg_run(input: &str) -> Result<SgRunArgs, String> {
    let tokens = tokenize(input)?;
    let mut idx = 0;

    // "sg" / "run" プレフィックスを読み飛ばし
    if tokens.get(idx).map(|s| s.as_str()) == Some("sg") {
        idx += 1;
    }
    if tokens.get(idx).map(|s| s.as_str()) == Some("run") {
        idx += 1;
    }

    let mut pattern: Option<String> = None;
    let mut lang = SupportedLanguage::Auto;
    let mut context_before: usize = 0;
    let mut context_after: usize = 0;
    let mut search_dir = String::new();

    while idx < tokens.len() {
        match tokens[idx].as_str() {
            "-p" | "--pattern" => {
                idx += 1;
                pattern = Some(
                    tokens
                        .get(idx)
                        .ok_or("-p の後にパターンが必要です")?
                        .clone(),
                );
                idx += 1;
            }
            "-l" | "--lang" => {
                idx += 1;
                let s = tokens
                    .get(idx)
                    .ok_or("-l の後に言語名が必要です")?;
                lang = SupportedLanguage::from_cli_str(s)
                    .ok_or_else(|| format!("不明な言語: {}", s))?;
                idx += 1;
            }
            "-C" | "--context" => {
                idx += 1;
                let n: usize = tokens
                    .get(idx)
                    .ok_or("-C の後に数値が必要です")?
                    .parse()
                    .map_err(|_| "-C の値が数値ではありません".to_string())?;
                context_before = n;
                context_after = n;
                idx += 1;
            }
            "-A" | "--after" => {
                idx += 1;
                context_after = tokens
                    .get(idx)
                    .ok_or("-A の後に数値が必要です")?
                    .parse()
                    .map_err(|_| "-A の値が数値ではありません".to_string())?;
                idx += 1;
            }
            "-B" | "--before" => {
                idx += 1;
                context_before = tokens
                    .get(idx)
                    .ok_or("-B の後に数値が必要です")?
                    .parse()
                    .map_err(|_| "-B の値が数値ではありません".to_string())?;
                idx += 1;
            }
            tok if !tok.starts_with('-') => {
                // 位置引数 = 検索ディレクトリ
                search_dir = tok.to_string();
                idx += 1;
            }
            tok => {
                return Err(format!("不明なオプション: {}", tok));
            }
        }
    }

    let pattern = pattern.ok_or("-p / --pattern が指定されていません")?;

    Ok(SgRunArgs {
        pattern,
        lang,
        context_before,
        context_after,
        search_dir,
    })
}

/// `sg` / `sg run` コマンドかどうかを判定する
pub fn is_sg_command(input: &str) -> bool {
    let s = input.trim();
    if s.starts_with("sg ") || s == "sg" {
        return true;
    }
    // "-p" で始まる場合もインターセプト
    s.starts_with("-p ") || s.starts_with("--pattern ")
}

/// クォートを考慮したトークナイザ
fn tokenize(s: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '"' => {
                // ダブルクォートで囲まれた文字列
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => {
                            // エスケープシーケンス
                            if let Some(esc) = chars.next() {
                                current.push(esc);
                            }
                        }
                        Some(ch) => current.push(ch),
                        None => return Err("クォートが閉じられていません".to_string()),
                    }
                }
            }
            '\'' => {
                // シングルクォートで囲まれた文字列（エスケープなし）
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => current.push(ch),
                        None => return Err("シングルクォートが閉じられていません".to_string()),
                    }
                }
            }
            ch => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}
