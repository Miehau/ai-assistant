mod approve;
mod download;
mod fetch;
mod request;

use crate::db::{Db, PreferenceOperations};
use crate::tools::{ToolError, ToolRegistry};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use reqwest::redirect::Policy;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::time::Duration;
use url::Url;

pub(crate) const PREF_ALLOWED_HOSTS: &str = "plugins.web.allowed_hosts";
pub(crate) const DEFAULT_MAX_BYTES: usize = 200_000;
pub(crate) const DEFAULT_MAX_DOWNLOAD_BYTES: usize = 10_485_760; // 10 MB
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 15_000;
pub(crate) const DEFAULT_USER_AGENT: &str = "ai-agent/1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AllowedHost {
    pub(crate) host: String,
    pub(crate) allow_private: bool,
    pub(crate) approved_at: i64,
}

pub fn register_web_tools(registry: &mut ToolRegistry, db: Db) -> Result<(), String> {
    approve::register_approve_tool(registry, db.clone())?;
    fetch::register_fetch_tool(registry, db.clone())?;
    request::register_request_tool(registry, db.clone())?;
    download::register_download_tool(registry, db)?;
    Ok(())
}

pub(crate) fn load_allowlist(db: &Db) -> Result<Vec<AllowedHost>, ToolError> {
    let raw = PreferenceOperations::get_preference(db, PREF_ALLOWED_HOSTS)
        .map_err(|err| ToolError::new(format!("Failed to load web allowlist: {err}")))?;
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&raw).map_err(|err| ToolError::new(format!("Invalid allowlist: {err}")))
}

pub(crate) fn save_allowlist(db: &Db, list: &[AllowedHost]) -> Result<(), ToolError> {
    let value = serde_json::to_string(list)
        .map_err(|err| ToolError::new(format!("Failed to serialize allowlist: {err}")))?;
    PreferenceOperations::set_preference(db, PREF_ALLOWED_HOSTS, &value)
        .map_err(|err| ToolError::new(format!("Failed to save web allowlist: {err}")))?;
    Ok(())
}

pub(crate) fn build_allowlist_map(list: &[AllowedHost]) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    for entry in list {
        map.insert(entry.host.clone(), entry.allow_private);
    }
    map
}

pub(crate) fn parse_method(input: &str) -> Result<Method, ToolError> {
    let normalized = input.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(ToolError::new("Method cannot be empty"));
    }
    Method::from_bytes(normalized.as_bytes())
        .map_err(|_| ToolError::new(format!("Invalid method '{input}'")))
}

pub(crate) fn parse_headers(args: &Value) -> Result<HeaderMap, ToolError> {
    let Some(value) = args.get("headers") else {
        return Ok(HeaderMap::new());
    };
    if value.is_null() {
        return Ok(HeaderMap::new());
    }
    let obj = value
        .as_object()
        .ok_or_else(|| ToolError::new("Invalid 'headers' (expected object)"))?;
    let mut headers = HeaderMap::new();
    for (key, raw_value) in obj {
        let value = raw_value
            .as_str()
            .ok_or_else(|| ToolError::new(format!("Invalid header value for '{key}'")))?;
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ToolError::new(format!("Invalid header name '{key}'")))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| ToolError::new(format!("Invalid header value for '{key}'")))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

pub(crate) fn headers_to_json(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        if let Ok(text) = value.to_str() {
            map.insert(name.to_string(), json!(text));
        }
    }
    Value::Object(map)
}

pub(crate) fn normalize_host_from_input(input: &str) -> Result<String, ToolError> {
    let url = parse_url(input)?;
    let host = url
        .host_str()
        .ok_or_else(|| ToolError::new("URL missing host"))?;
    normalize_host(host)
}

pub(crate) fn normalize_host(host: &str) -> Result<String, ToolError> {
    let normalized = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(ToolError::new("Host is empty"));
    }
    Ok(normalized)
}

pub(crate) fn parse_url(input: &str) -> Result<Url, ToolError> {
    match Url::parse(input) {
        Ok(url) => Ok(url),
        Err(_) => {
            let with_scheme = format!("https://{input}");
            Url::parse(&with_scheme).map_err(|err| ToolError::new(format!("Invalid URL: {err}")))
        }
    }
}

pub(crate) fn ensure_host_allowed(list: &[AllowedHost], host: &str) -> Result<(), ToolError> {
    let entry = list.iter().find(|entry| entry.host == host);
    let Some(entry) = entry else {
        return Err(ToolError::new(format!(
            "Host not approved: {host}. Use web.approve_domain first."
        )));
    };
    if is_private_host(host) && !entry.allow_private {
        return Err(ToolError::new(
            "Private/local host blocked. Re-approve with allow_private=true.",
        ));
    }
    Ok(())
}

pub(crate) fn is_private_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_private_ip(ip);
    }
    false
}

pub(crate) fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => {
            addr.is_private()
                || addr.is_loopback()
                || addr.is_link_local()
                || addr.is_unspecified()
                || addr.is_multicast()
                || addr.is_broadcast()
        }
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unique_local()
                || addr.is_unicast_link_local()
                || addr.is_unspecified()
                || addr.is_multicast()
        }
    }
}

pub(crate) fn build_client(
    timeout_ms: u64,
    user_agent: &str,
    allowlist: &HashMap<String, bool>,
    base_host: Option<&str>,
    same_host_only: bool,
) -> Result<Client, ToolError> {
    let allowlist = allowlist.clone();
    let base_host = base_host.map(|host| host.to_string());
    let policy = Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.stop();
        }
        let host = attempt.url().host_str().unwrap_or("");
        let host = match normalize_host(host) {
            Ok(host) => host,
            Err(_) => return attempt.stop(),
        };
        if same_host_only {
            if let Some(base) = &base_host {
                if &host != base {
                    return attempt.stop();
                }
            }
        }
        match allowlist.get(&host) {
            Some(allow_private) => {
                if is_private_host(&host) && !*allow_private {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }
            None => attempt.stop(),
        }
    });

    Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .user_agent(user_agent)
        .redirect(policy)
        .build()
        .map_err(|err| ToolError::new(format!("Failed to build client: {err}")))
}

pub(crate) fn read_limited_body(
    response: reqwest::blocking::Response,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), ToolError> {
    use std::io::Read;
    let mut body = Vec::new();
    let mut limited = response.take((max_bytes + 1) as u64);
    limited
        .read_to_end(&mut body)
        .map_err(|err| ToolError::new(format!("Failed to read response: {err}")))?;
    let truncated = body.len() > max_bytes;
    if truncated {
        body.truncate(max_bytes);
    }
    Ok((body, truncated))
}

pub(crate) fn is_html_content(content_type: &str, body: &str) -> bool {
    content_type.to_ascii_lowercase().contains("text/html") || body.contains("<html")
}

pub(crate) fn is_text_content(content_type: &str) -> bool {
    let lower = content_type.to_ascii_lowercase();
    lower.starts_with("text/") || lower.contains("json") || lower.contains("xml")
}

pub(crate) fn extract_title(document: &Html) -> String {
    let selector = match Selector::parse("title") {
        Ok(selector) => selector,
        Err(_) => return String::new(),
    };
    document
        .select(&selector)
        .next()
        .map(|node| node.text().collect::<Vec<_>>().join(" ").trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn extract_text(document: &Html) -> String {
    let selector = Selector::parse("body").ok();
    let text_iter = if let Some(selector) = selector {
        document
            .select(&selector)
            .next()
            .map(|node| node.text().collect::<Vec<_>>())
            .unwrap_or_default()
    } else {
        document.root_element().text().collect::<Vec<_>>()
    };

    let mut result = String::new();
    for chunk in text_iter {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(trimmed);
    }
    result
}

pub(crate) fn extract_links_from_document(
    document: &Html,
    base_url: &Url,
    same_host_only: bool,
    max_links: usize,
) -> Vec<String> {
    let selector = match Selector::parse("a[href]") {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };
    let mut seen = HashSet::new();
    let base_host = base_url.host_str().unwrap_or("").to_ascii_lowercase();
    let mut links = Vec::new();

    for node in document.select(&selector) {
        if links.len() >= max_links {
            break;
        }
        let href = match node.value().attr("href") {
            Some(href) => href.trim(),
            None => continue,
        };
        if href.is_empty() {
            continue;
        }
        let resolved = match base_url.join(href) {
            Ok(url) => url,
            Err(_) => continue,
        };
        let scheme = resolved.scheme();
        if scheme != "http" && scheme != "https" {
            continue;
        }
        let host = resolved.host_str().unwrap_or("").to_ascii_lowercase();
        if same_host_only && host != base_host {
            continue;
        }
        let link = resolved.to_string();
        if seen.insert(link.clone()) {
            links.push(link);
        }
    }
    links
}

pub(crate) fn require_string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| ToolError::new(format!("Missing or invalid '{key}'")))
}
