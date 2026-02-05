// src-tauri/src/files/text.rs
use serde_json::json;
use std::io;
use std::str;

pub struct TextProcessor;

impl TextProcessor {
    // Validate text data
    pub fn validate_text(data: &[u8]) -> Result<bool, io::Error> {
        // Check if the data is valid UTF-8
        match str::from_utf8(data) {
            Ok(_) => Ok(true),
            Err(e) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid UTF-8 text: {}", e),
            )),
        }
    }

    // Extract metadata and content information from text
    pub fn extract_metadata(data: &[u8]) -> Result<serde_json::Value, io::Error> {
        // Convert bytes to string
        let text = match str::from_utf8(data) {
            Ok(s) => s,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid UTF-8 text: {}", e),
                ))
            }
        };

        // Count lines, words, and characters
        let line_count = text.lines().count();
        let word_count = text.split_whitespace().count();
        let char_count = text.chars().count();

        // Detect content type (markdown, code, etc.)
        let content_type = Self::detect_content_type(text);

        // Generate a preview (first few lines)
        let preview = Self::generate_preview(text, 5);

        let metadata = json!({
            "line_count": line_count,
            "word_count": word_count,
            "char_count": char_count,
            "content_type": content_type,
            "preview": preview,
            "size_bytes": data.len(),
        });

        Ok(metadata)
    }

    // Detect the likely content type of the text
    fn detect_content_type(text: &str) -> &'static str {
        // Simple heuristics to guess the content type

        // Check for markdown indicators
        if text.contains("#") && (text.contains("##") || text.contains("*")) {
            return "markdown";
        }

        // Check for common code indicators
        if text.contains("{")
            && text.contains("}")
            && (text.contains("function")
                || text.contains("class")
                || text.contains("import")
                || text.contains("const")
                || text.contains("var")
                || text.contains("let"))
        {
            return "code";
        }

        // Check for JSON
        if (text.starts_with("{") && text.ends_with("}"))
            || (text.starts_with("[") && text.ends_with("]"))
        {
            return "json";
        }

        // Check for HTML
        if text.contains("<html")
            || text.contains("<!DOCTYPE")
            || (text.contains("<")
                && text.contains(">")
                && (text.contains("<div") || text.contains("<p") || text.contains("<span")))
        {
            return "html";
        }

        // Default
        "plain_text"
    }

    // Generate a preview of the text (first n lines)
    fn generate_preview(text: &str, line_count: usize) -> String {
        let preview: String = text
            .lines()
            .take(line_count)
            .collect::<Vec<&str>>()
            .join("\n");

        if preview.len() < text.len() {
            format!("{}...", preview)
        } else {
            preview
        }
    }

    // Extract code from text if it contains code blocks
    pub fn extract_code(data: &[u8]) -> Result<Vec<(String, String)>, io::Error> {
        // Convert bytes to string
        let text = match str::from_utf8(data) {
            Ok(s) => s,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid UTF-8 text: {}", e),
                ))
            }
        };

        let mut code_blocks = Vec::new();

        // Extract markdown code blocks
        let mut in_code_block = false;
        let mut current_language = String::new();
        let mut current_code = String::new();

        for line in text.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    code_blocks.push((current_language.clone(), current_code.clone()));
                    current_code.clear();
                    in_code_block = false;
                } else {
                    // Start of code block
                    in_code_block = true;
                    current_language = line.trim_start_matches("```").trim().to_string();
                    if current_language.is_empty() {
                        current_language = "text".to_string();
                    }
                }
            } else if in_code_block {
                current_code.push_str(line);
                current_code.push('\n');
            }
        }

        Ok(code_blocks)
    }
}
