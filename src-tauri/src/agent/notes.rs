use crate::db::{Db, ModelOperations, PreferenceOperations};
use crate::llm::{
    complete_anthropic_with_output_format,
    complete_claude_cli,
    complete_openai,
    complete_openai_compatible,
    json_schema_output_format,
    LlmMessage,
    StreamResult,
};
use crate::tools::vault::get_vault_root;
use chrono::Local;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct NoteTriageRaw {
    capture: Option<bool>,
    action: Option<String>,
    topic_title: Option<String>,
    topic: Option<String>,
    summary: Option<Vec<String>>,
    links: Option<Vec<String>>,
}

#[derive(Debug)]
struct NoteTriage {
    capture: bool,
    topic_title: String,
    summary: Vec<String>,
    links: Vec<String>,
}

const NOTE_TRIAGE_SYSTEM: &str = "You decide whether an exchange is worth a durable note. \
Skip trivial or ephemeral queries (weather, time, quick definitions, single-fact lookups). \
Capture durable knowledge (design decisions, plans, reusable facts). \
Return ONLY valid JSON.";

const NOTE_TRIAGE_PROMPT: &str = "Analyze the exchange and output JSON with keys:\n\
capture: boolean\n\
topic_title: short, stable topic (3-8 words)\n\
summary: array of 1-5 concise bullet strings\n\
links: array of Obsidian-style links like [[Thing]] (only if real)\n\
If capture is false, set topic_title to empty string and summary/links to empty arrays.\n\
\n\
Exchange:\n\
User:\n\
{user_message}\n\
\n\
Assistant:\n\
{assistant_message}\n";

pub fn capture_topic_note(
    db: Db,
    provider: String,
    model: String,
    custom_backend: Option<(String, Option<String>)>,
    user_message: String,
    assistant_message: String,
) -> Result<(), String> {
    let root = match get_vault_root(&db) {
        Ok(root) => root,
        Err(err) => {
            log::warn!(
                "[notes] capture skipped: vault root unavailable: {}",
                err.message
            );
            return Ok(());
        }
    };

    if user_message.trim().is_empty() || assistant_message.trim().is_empty() {
        return Ok(());
    }

    let preferred_model = PreferenceOperations::get_preference(&db, "agent.notes.model")
        .ok()
        .flatten()
        .unwrap_or_default();
    let model = if preferred_model.trim().is_empty() {
        model
    } else {
        preferred_model
    };

    let triage = match triage_note(
        &db,
        &provider,
        &model,
        custom_backend,
        &user_message,
        &assistant_message,
    ) {
        Ok(result) => result,
        Err(err) => {
            log::warn!(
                "[notes] triage failed: provider={} model={} error={}",
                provider,
                model,
                err
            );
            return Ok(());
        }
    };

    if !triage.capture {
        return Ok(());
    }

    let topic_title = triage.topic_title.trim();
    if topic_title.is_empty() {
        return Ok(());
    }

    let date = Local::now().format("%Y-%m-%d").to_string();
    let time_label = Local::now().format("%H:%M").to_string();
    let sanitized_title = sanitize_filename(topic_title);
    if sanitized_title.is_empty() {
        return Ok(());
    }

    let candidate = find_today_topic_file(&root, &date, &topic_title)?;
    let note_path = if let Some(existing) = candidate {
        existing
    } else {
        let filename = format!("{date} {sanitized_title}.md");
        root.join(filename)
    };

    if note_path.exists() {
        append_note_entry(&note_path, &time_label, &triage.summary, &triage.links)?;
    } else {
        create_note(&note_path, topic_title, &time_label, &triage.summary, &triage.links)?;
    }

    Ok(())
}

fn triage_note(
    db: &Db,
    provider: &str,
    model: &str,
    custom_backend: Option<(String, Option<String>)>,
    user_message: &str,
    assistant_message: &str,
) -> Result<NoteTriage, String> {
    let prompt = NOTE_TRIAGE_PROMPT
        .replace("{user_message}", &truncate_text(user_message.trim(), 1200))
        .replace("{assistant_message}", &truncate_text(assistant_message.trim(), 1400));
    let messages = vec![LlmMessage {
        role: "user".to_string(),
        content: json!(prompt),
    }];

    let output_format = if provider == "anthropic" || provider == "claude_cli" {
        Some(json_schema_output_format(note_triage_schema()))
    } else {
        None
    };

    let response = call_note_llm(
        db,
        provider,
        model,
        custom_backend,
        NOTE_TRIAGE_SYSTEM,
        &messages,
        output_format,
    )?;

    let json_text = extract_json(&response.content);
    let raw: NoteTriageRaw = serde_json::from_str(&json_text)
        .map_err(|err| format!("Invalid note triage JSON: {err}"))?;

    let capture = raw
        .capture
        .or_else(|| match raw.action.as_deref() {
            Some("skip") => Some(false),
            Some("capture") | Some("keep") => Some(true),
            _ => None,
        })
        .unwrap_or(false);

    let topic_title = raw
        .topic_title
        .or(raw.topic)
        .unwrap_or_default()
        .trim()
        .to_string();

    let summary = raw
        .summary
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .take(5)
        .collect::<Vec<_>>();

    let links = raw
        .links
        .unwrap_or_default()
        .into_iter()
        .map(|line| normalize_link(&line))
        .filter(|line| !line.is_empty())
        .take(8)
        .collect::<Vec<_>>();

    Ok(NoteTriage {
        capture,
        topic_title,
        summary,
        links,
    })
}

fn call_note_llm(
    db: &Db,
    provider: &str,
    model: &str,
    custom_backend: Option<(String, Option<String>)>,
    system_prompt: &str,
    messages: &[LlmMessage],
    output_format: Option<Value>,
) -> Result<StreamResult, String> {
    let client = Client::new();
    let provider = provider.to_lowercase();

    let prepared_messages = if provider == "anthropic" || provider == "claude_cli" {
        messages.to_vec()
    } else {
        let mut prepared = messages.to_vec();
        if !system_prompt.trim().is_empty() {
            prepared.insert(
                0,
                LlmMessage {
                    role: "system".to_string(),
                    content: json!(system_prompt),
                },
            );
        }
        prepared
    };

    match provider.as_str() {
        "openai" => {
            let api_key = ModelOperations::get_api_key(db, "openai")
                .ok()
                .flatten()
                .unwrap_or_default();
            if api_key.is_empty() {
                Err("Missing OpenAI API key".to_string())
            } else {
                complete_openai(
                    &client,
                    &api_key,
                    "https://api.openai.com/v1/chat/completions",
                    model,
                    &prepared_messages,
                )
            }
        }
        "anthropic" => {
            let api_key = ModelOperations::get_api_key(db, "anthropic")
                .ok()
                .flatten()
                .unwrap_or_default();
            if api_key.is_empty() {
                Err("Missing Anthropic API key".to_string())
            } else {
                complete_anthropic_with_output_format(
                    &client,
                    &api_key,
                    model,
                    Some(system_prompt),
                    &prepared_messages,
                    output_format,
                )
            }
        }
        "deepseek" => {
            let api_key = ModelOperations::get_api_key(db, "deepseek")
                .ok()
                .flatten()
                .unwrap_or_default();
            if api_key.is_empty() {
                Err("Missing DeepSeek API key".to_string())
            } else {
                complete_openai_compatible(
                    &client,
                    Some(&api_key),
                    "https://api.deepseek.com/chat/completions",
                    model,
                    &prepared_messages,
                )
            }
        }
        "claude_cli" => complete_claude_cli(model, Some(system_prompt), &prepared_messages, output_format),
        "custom" | "ollama" => {
            let (url, api_key) = custom_backend.unwrap_or_default();
            if url.is_empty() {
                Err("Missing custom backend URL".to_string())
            } else {
                complete_openai_compatible(
                    &client,
                    api_key.as_deref(),
                    &url,
                    model,
                    &prepared_messages,
                )
            }
        }
        _ => Err(format!("Unsupported provider: {provider}")),
    }
}

fn note_triage_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "capture": { "type": "boolean" },
            "topic_title": { "type": "string" },
            "summary": {
                "type": "array",
                "items": { "type": "string" }
            },
            "links": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["capture", "topic_title", "summary", "links"],
        "additionalProperties": false
    })
}

fn find_today_topic_file(root: &Path, date: &str, topic_title: &str) -> Result<Option<PathBuf>, String> {
    let candidates = list_today_files(root, date)?;
    let target = normalize_title(topic_title);
    for path in candidates {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(h1) = extract_h1(&content) {
                if normalize_title(&h1) == target {
                    return Ok(Some(path));
                }
            }
        }
    }
    Ok(None)
}

fn list_today_files(root: &Path, date: &str) -> Result<Vec<PathBuf>, String> {
    if let Ok(output) = Command::new("rg")
        .arg("--files")
        .arg("-g")
        .arg(format!("{date}*.md"))
        .current_dir(root)
        .output()
    {
        let code = output.status.code().unwrap_or(0);
        if code == 0 || code == 1 {
            let mut results = Vec::new();
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.contains('/') || trimmed.contains('\\') {
                    continue;
                }
                results.push(root.join(trimmed));
            }
            if !results.is_empty() || code == 0 {
                return Ok(results);
            }
        }
    }

    let mut results = Vec::new();
    let entries = fs::read_dir(root).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(date) && name.ends_with(".md") {
                results.push(path);
            }
        }
    }
    Ok(results)
}

fn create_note(
    path: &Path,
    topic_title: &str,
    time_label: &str,
    summary: &[String],
    links: &[String],
) -> Result<(), String> {
    let mut content = String::new();
    content.push_str("# ");
    content.push_str(topic_title.trim());
    content.push_str("\n\n");
    content.push_str("## ");
    content.push_str(time_label);
    content.push_str("\n");
    content.push_str(&format_section(summary, links));
    fs::write(path, content).map_err(|err| err.to_string())
}

fn append_note_entry(
    path: &Path,
    time_label: &str,
    summary: &[String],
    links: &[String],
) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())?;
    let mut content = String::new();
    content.push_str("\n\n## ");
    content.push_str(time_label);
    content.push_str("\n");
    content.push_str(&format_section(summary, links));
    file.write_all(content.as_bytes())
        .map_err(|err| err.to_string())
}

fn format_section(summary: &[String], links: &[String]) -> String {
    let mut content = String::new();
    if !summary.is_empty() {
        content.push_str("Summary\n");
        for line in summary {
            content.push_str("- ");
            content.push_str(line.trim());
            content.push('\n');
        }
    }

    if !links.is_empty() {
        if !summary.is_empty() {
            content.push('\n');
        }
        content.push_str("Links\n");
        for link in links {
            content.push_str("- ");
            content.push_str(link.trim());
            content.push('\n');
        }
    }

    if summary.is_empty() && links.is_empty() {
        content.push_str("Summary\n- (no details)\n");
    }

    content
}

fn extract_h1(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return Some(trimmed.trim_start_matches("# ").trim().to_string());
        }
    }
    None
}

fn normalize_title(input: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in input.trim().to_lowercase().chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn sanitize_filename(input: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in input.chars() {
        let ch = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            _ => ch,
        };
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn normalize_link(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
        return trimmed.to_string();
    }
    format!("[[{}]]", trimmed.trim_matches(&['[', ']'][..]))
}

fn extract_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return extract_json_span(trimmed).unwrap_or_else(|| trimmed.to_string());
    }

    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or("");
    if !first_line.starts_with("```") {
        return trimmed.to_string();
    }

    let mut json_lines: Vec<&str> = lines.collect();
    if let Some(last) = json_lines.last() {
        if last.trim().starts_with("```") {
            json_lines.pop();
        }
    }

    let extracted = json_lines.join("\n").trim().to_string();
    extract_json_span(&extracted).unwrap_or(extracted)
}

fn extract_json_span(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = None;
    let mut end = None;
    for (idx, ch) in bytes.iter().enumerate() {
        if *ch == b'{' {
            start = Some(idx);
            break;
        }
    }
    for (idx, ch) in bytes.iter().enumerate().rev() {
        if *ch == b'}' {
            end = Some(idx);
            break;
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let slice = &text[s..=e];
            Some(slice.trim().to_string())
        }
        _ => None,
    }
}

fn truncate_text(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}
