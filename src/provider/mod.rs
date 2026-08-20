pub mod openrouter;
pub mod types;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

pub use types::*;

use crate::config::Config;

pub type DeltaSink = UnboundedSender<StreamDelta>;

/// Free models wrap their answer in our JSON text-protocol

#[derive(Default)]
pub struct StreamingAnswerExtractor {
    buf: String,
    emitted: usize,
}

impl StreamingAnswerExtractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        let current = self.current_answer();
        if current.len() >= self.emitted {
            let new = current[self.emitted..].to_string();
            self.emitted = current.len();
            new
        } else {
            self.emitted = current.len();
            String::new()
        }
    }

    fn current_answer(&self) -> String {
        if self.buf.contains("<tool_call>")
            || self.buf.contains("<|tool_call")
            || self.buf.contains("<|")
            || self.buf.contains("<invoke")
            || self.buf.contains("<function")
            || self.buf.contains("<dots")
        {
            return String::new();
        }

        if self.buf.contains('{') || self.buf.contains("```") || self.buf.contains("~~~") {
            return extract_growing_content(&self.buf).unwrap_or_default();
        }
        self.buf.clone()
    }
}

fn extract_growing_content(buf: &str) -> Option<String> {
    for key in ["content", "text"] {
        let pat = format!("\"{key}\"");
        let kpos = buf.find(&pat)?;
        let mut i = kpos + pat.len();
        while i < buf.len() && (buf.as_bytes()[i] == b':' || buf.as_bytes()[i] == b' ') {
            i += 1;
        }
        if i >= buf.len() || buf.as_bytes()[i] != b'"' {
            continue;
        }
        let start = i + 1;
        let rest = &buf[start..];
        let mut s = rest.to_string();

        if let Some(stripped) = s.strip_suffix("```") {
            s = stripped.to_string();
        }
        s = s.trim_end().to_string();

        if let Some(stripped) = s.strip_suffix("\"}\"") {
            s = format!("{stripped}\"");
        }
        while s.ends_with('}') || s.ends_with(']') || s.ends_with(',') {
            s.pop();
        }
        if s.ends_with('"') {
            let bytes = s.as_bytes();
            if bytes.len() >= 2 && bytes[bytes.len() - 2] != b'\\' {
                s.pop();
            }
        }
        return Some(unescape_json(&s));
    }
    None
}

fn unescape_json(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

pub fn clean_answer(text: &str) -> String {
    use regex::Regex;
    let re = Regex::new(r"(?im)^\s*((user|response|content|prompt)\s*)?safety\s*[:=]\s*\w+\s*$")
        .expect("valid safety regex");
    text.lines()
        .filter(|line| !re.is_match(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod extractor_tests {
    use super::*;

    #[test]
    fn grows_with_stream() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(ex.push("{\"type\":\"final\",\"content\":\"Hel"), "Hel");
        assert_eq!(ex.push("lo\"}"), "lo");

        assert_eq!(ex.push(""), "");
    }

    #[test]
    fn plain_prose_streams() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(ex.push("Hi"), "Hi");
        assert_eq!(ex.push(" there"), " there");
    }

    #[test]
    fn tool_call_yields_nothing() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(
            ex.push("{\"type\":\"tool\",\"tool\":\"read_file\",\"arguments\":{\"path\":\"a\"}}"),
            ""
        );
    }

    #[test]
    fn tolerates_spaces_around_colon() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(ex.push("{\"type\": \"final\", \"content\": \"Hel"), "Hel");
        assert_eq!(ex.push("lo\"}"), "lo");
    }

    #[test]
    fn does_not_leak_raw_json() {
        let mut ex = StreamingAnswerExtractor::new();

        assert_eq!(ex.push("{\"type\":\"final\",\"content\":\""), "");

        let mut ex2 = StreamingAnswerExtractor::new();
        assert_eq!(ex2.push("{\"2+2 equals 4."), "");
    }

    #[test]
    fn handles_fenced_json_without_leaking() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(ex.push("```json\n"), "");
        assert_eq!(ex.push("{\"type\":\"final\",\"content\":\"Hel"), "Hel");
        assert_eq!(ex.push("lo\"}\n```"), "lo");
    }

    #[test]
    fn does_not_leak_pipe_tool_markers() {
        let mut ex = StreamingAnswerExtractor::new();
        assert_eq!(
            ex.push("<|tool_call_start|>[write_file(path='a')]<|tool_call_end|>"),
            ""
        );

        assert_eq!(ex.push(""), "");
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;

    fn model(&self) -> &str;

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResult, ProviderError>;

    async fn stream(
        &self,
        req: CompletionRequest,
        deltas: DeltaSink,
    ) -> Result<CompletionResult, ProviderError>;
}

pub fn create_provider(config: &Config) -> Result<Arc<dyn Provider>, ProviderError> {
    match config.provider.as_str() {
        "openrouter" | "openai" | "anthropic" => Ok(Arc::new(openrouter::OpenRouterProvider::new(
            config.provider.clone(),
            config.api_key.clone(),
            config.base_url.clone(),
            config.model.clone(),
        ))),
        other => Err(ProviderError::Config(format!(
            "unknown provider `{other}`; supported: openrouter, openai, anthropic"
        ))),
    }
}

pub fn parse_decision(text: &str) -> Result<FinishDecision, ProviderError> {
    let text = text.trim();

    if let Some(decision) = try_json(text)
        .or_else(|| try_code_block(text))
        .or_else(|| try_object(text))
        .or_else(|| try_tool_call_xml(text))
        .or_else(|| try_invoke_xml(text))
        .or_else(|| try_tool_use_xml(text))
        .or_else(|| try_inline_content(text))
        .or_else(|| try_tool_marker(text))
    {
        return Ok(decision);
    }

    if text.is_empty() {
        return Err(ProviderError::Malformed(
            "model returned an empty response".into(),
        ));
    }

    Ok(FinishDecision::Final(text.to_string()))
}

fn try_json(text: &str) -> Option<FinishDecision> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    decision_from_value(value)
}

fn try_code_block(text: &str) -> Option<FinishDecision> {
    let start = text.find("```json")? + "```json".len();
    let rest = &text[start..];
    let end = rest.find("```")?;
    let json = rest[..end].trim();
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    decision_from_value(value)
}

fn try_object(text: &str) -> Option<FinishDecision> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start >= end {
        return None;
    }
    let json = &text[start..=end];
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    decision_from_value(value)
}

fn try_tool_marker(text: &str) -> Option<FinishDecision> {
    let start_marker = "<|tool_call_start|>";
    let end_marker = "<|tool_call_end|>";

    let start = text.find(start_marker)? + start_marker.len();
    let end = text.find(end_marker)?;
    if end <= start {
        return None;
    }
    let call = text[start..end].trim();
    let call = call
        .strip_prefix('[')
        .and_then(|c| c.strip_suffix(']'))
        .unwrap_or(call)
        .trim();
    let open = call.find('(')?;
    let close = call.rfind(')')?;
    let tool = call[..open].trim().to_string();
    let args_text = &call[open + 1..close];
    let arguments = parse_python_args(args_text)?;
    Some(FinishDecision::ToolCall {
        name: tool,
        arguments,
    })
}

fn try_tool_call_xml(text: &str) -> Option<FinishDecision> {
    let open = "<tool_call>";
    let close = "</tool_call>";
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    if end <= start {
        return None;
    }
    let inner = &text[start..end];

    let name_end = inner.find('<').unwrap_or(inner.len());
    let name = inner[..name_end].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let mut arguments = serde_json::Map::new();
    let mut rest = &inner[name_end..];
    while let Some(ak_open) = rest.find("<arg_key>") {
        let key_start = ak_open + "<arg_key>".len();
        let key_end = rest[key_start..].find("</arg_key>")? + key_start;
        let key = rest[key_start..key_end].trim().to_string();

        let after_key = &rest[key_end + "</arg_key>".len()..];
        let av_open = after_key.find("<arg_value>")? + "<arg_value>".len();
        let av_end = after_key[av_open..].find("</arg_value>")? + av_open;
        let value = after_key[av_open..av_end].to_string();

        arguments.insert(key, serde_json::Value::String(value));
        rest = &after_key[av_end + "</arg_value>".len()..];
    }

    Some(FinishDecision::ToolCall {
        name,
        arguments: serde_json::Value::Object(arguments),
    })
}

fn try_invoke_xml(text: &str) -> Option<FinishDecision> {
    let open = "<invoke";
    let start = text.find(open)?;
    let end_marker = "</invoke>";
    let end = text[start..].find(end_marker)? + start + end_marker.len();
    if end <= start {
        return None;
    }
    let block = &text[start..end];

    let name = attr_value(block, "name")?;
    if name.is_empty() {
        return None;
    }

    let mut arguments = serde_json::Map::new();
    let mut rest = block;
    while let Some(p_open) = rest.find("<parameter") {
        let tag_end = rest[p_open..].find('>')? + p_open;
        let after_tag = &rest[tag_end + 1..];
        let close_pos = after_tag.find("</parameter>")?;
        let inner = after_tag[..close_pos].trim().to_string();
        let tag = &rest[p_open..=tag_end];

        let key = attr_value(tag, "name")
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| first_attr_name(tag).unwrap_or_else(|| "value".to_string()));

        let value = if inner.is_empty() {
            attr_value(tag, "command")
                .or_else(|| attr_value(tag, "namecommand"))
                .unwrap_or_default()
        } else {
            inner
        };

        arguments.insert(key, serde_json::Value::String(value));
        rest = &after_tag[close_pos + "</parameter>".len()..];
    }

    if name == "run_command" && !arguments.contains_key("command") {
        if let Some(v) = arguments.values().next().cloned() {
            arguments.insert("command".to_string(), v);
        }
    }

    Some(FinishDecision::ToolCall {
        name,
        arguments: serde_json::Value::Object(arguments),
    })
}

fn attr_value(haystack: &str, attr: &str) -> Option<String> {
    let marker = format!("{attr}=\"");
    let s = haystack.find(&marker)? + marker.len();
    let end = haystack[s..].find('"')? + s;
    Some(haystack[s..end].to_string())
}

fn first_attr_name(tag: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() {
            let start = i;
            let mut j = i;
            while j < bytes.len() && bytes[j] != b'=' && !bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let name = &tag[start..j];
            if !name.is_empty() {
                return Some(name.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

fn try_tool_use_xml(text: &str) -> Option<FinishDecision> {
    let open = "<tool_use";
    let start = text.find(open)?;
    let end_marker = "</tool_use>";
    let end = text[start..].find(end_marker)? + start + end_marker.len();
    if end <= start {
        return None;
    }
    let block = &text[start..end];

    let name = attr_value(block, "name")
        .filter(|n| !n.is_empty())
        .or_else(|| {
            let fn_pos = block.find("<function")?;
            let fn_tag_end = block[fn_pos..].find('>')? + fn_pos;
            attr_value(&block[fn_pos..=fn_tag_end], "name")
        })?;

    let mut arguments = serde_json::Map::new();
    let mut rest = block;
    while let Some(p_open) = rest.find("<parameter") {
        let tag_end = rest[p_open..].find('>')? + p_open;
        let after_tag = &rest[tag_end + 1..];
        let close_pos = after_tag.find("</parameter>")?;
        let inner = after_tag[..close_pos].trim().to_string();
        let tag = &rest[p_open..=tag_end];
        let key = attr_value(tag, "name")
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| first_attr_name(tag).unwrap_or_else(|| "value".to_string()));
        let value = if inner.is_empty() {
            attr_value(tag, "command")
                .or_else(|| attr_value(tag, "namecommand"))
                .unwrap_or_default()
        } else {
            inner
        };
        arguments.insert(key, serde_json::Value::String(value));
        rest = &after_tag[close_pos + "</parameter>".len()..];
    }

    if name == "run_command" && !arguments.contains_key("command") {
        if let Some(v) = arguments.values().next().cloned() {
            arguments.insert("command".to_string(), v);
        }
    }

    Some(FinishDecision::ToolCall {
        name,
        arguments: serde_json::Value::Object(arguments),
    })
}

fn decision_from_value(value: serde_json::Value) -> Option<FinishDecision> {
    let obj = value.as_object()?;

    let kind = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("response_type").and_then(|v| v.as_str()));

    match kind {
        Some("final" | "answer") => {
            let content = obj
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("text").and_then(|v| v.as_str()))
                .unwrap_or("Done.")
                .to_string();
            Some(FinishDecision::Final(content))
        }
        Some("tool" | "tool_call") => {
            let tool = obj.get("tool").and_then(|v| v.as_str())?.to_string();
            let arguments = obj
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Some(FinishDecision::ToolCall {
                name: tool,
                arguments,
            })
        }

        _ => {
            if let Some(content) = obj
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("text").and_then(|v| v.as_str()))
                .or_else(|| obj.get("answer").and_then(|v| v.as_str()))
            {
                return Some(FinishDecision::Final(content.to_string()));
            }

            let tool = obj
                .get("tool")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("name").and_then(|v| v.as_str()));
            if let Some(tool) = tool {
                let arguments = obj
                    .get("arguments")
                    .or_else(|| obj.get("parameters"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                return Some(FinishDecision::ToolCall {
                    name: tool.to_string(),
                    arguments,
                });
            }
            None
        }
    }
}

fn try_inline_content(text: &str) -> Option<FinishDecision> {
    for key in ["content", "text", "answer"] {
        let pat = format!("\"{key}\"");
        let kpos = text.find(&pat)?;
        let mut i = kpos + pat.len();
        while i < text.len() && (text.as_bytes()[i] == b':' || text.as_bytes()[i] == b' ') {
            i += 1;
        }
        if i >= text.len() || text.as_bytes()[i] != b'"' {
            continue;
        }
        let start = i + 1;
        if let Some(end) = text[start..].find('"') {
            if start + end > 0 && text.as_bytes()[start + end - 1] == b'\\' {
                continue;
            }
            return Some(FinishDecision::Final(unescape_json(
                &text[start..start + end],
            )));
        }
    }
    None
}

fn parse_python_args(input: &str) -> Option<serde_json::Value> {
    let mut object = serde_json::Map::new();
    let mut current = String::new();
    let mut parts = Vec::new();
    let mut quote = None;

    for character in input.chars() {
        match character {
            '\'' | '"' if quote.is_none() => {
                quote = Some(character);
                current.push(character);
            }
            '\'' | '"' if quote == Some(character) => {
                quote = None;
                current.push(character);
            }
            ',' if quote.is_none() => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    for part in parts {
        let (key, value) = part.split_once('=')?;
        let key = key.trim();
        let value = value.trim();
        let value = value
            .strip_prefix('\'')
            .and_then(|v| v.strip_suffix('\''))
            .or_else(|| value.strip_prefix('"').and_then(|v| v.strip_suffix('"')))?;
        object.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }

    Some(serde_json::Value::Object(object))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_final_json() {
        let d = parse_decision(r#"{"type":"final","content":"hello"}"#).unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "hello"));
    }

    #[test]
    fn parses_tool_json() {
        let d = parse_decision(r#"{"type":"tool","tool":"read_file","arguments":{"path":"x"}}"#)
            .unwrap();
        match d {
            FinishDecision::ToolCall { name, .. } => assert_eq!(name, "read_file"),
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_fenced_block() {
        let d = parse_decision("```json\n{\"type\":\"final\",\"content\":\"yo\"}\n```").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "yo"));
    }

    #[test]
    fn parses_prose_with_json() {
        let d = parse_decision("Sure! {\"type\":\"final\",\"content\":\"hi\"} done").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "hi"));
    }

    #[test]
    fn treats_plain_text_as_final() {
        let d = parse_decision("Just do it").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "Just do it"));
    }

    #[test]
    fn parses_claude_xml_tool_call() {
        let d = parse_decision(
            "<tool_call>list_files<arg_key>path</arg_key><arg_value>src</arg_value></tool_call>",
        )
        .unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "list_files");
                assert_eq!(arguments["path"], "src");
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_xml_tool_call_with_multiple_args() {
        let d = parse_decision(
            "<tool_call>edit_file<arg_key>path</arg_key><arg_value>a.rs</arg_value>\
             <arg_key>old</arg_key><arg_value>foo</arg_value>\
             <arg_key>new</arg_key><arg_value>bar</arg_value></tool_call>",
        )
        .unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "edit_file");
                assert_eq!(arguments["path"], "a.rs");
                assert_eq!(arguments["old"], "foo");
                assert_eq!(arguments["new"], "bar");
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_json_with_spaces_and_no_type() {
        let d = parse_decision("{\"content\": \"hello world\"}").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "hello world"));
    }

    #[test]
    fn parses_typed_json_with_spaces() {
        let d = parse_decision("{\"type\": \"final\", \"content\": \"hi\"}").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "hi"));
    }

    #[test]
    fn extracts_inline_content_from_loose_json() {
        let d = parse_decision("{\"answer\":\"the result\"}").unwrap();
        assert!(matches!(d, FinishDecision::Final(s) if s == "the result"));
    }

    #[test]
    fn parses_name_parameters_format() {
        let d = parse_decision(r#"{"name":"run_command","parameters":{"command":"ls"}}"#).unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "run_command");
                assert_eq!(arguments["command"], "ls");
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_pipe_tool_call_markers() {
        let d = parse_decision(
            "<|tool_call_start|>[write_file(path='a.rs', content='x')]<|tool_call_end|>",
        )
        .unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "write_file");
                assert_eq!(arguments["path"], "a.rs");
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_pipe_markers_without_brackets() {
        let d =
            parse_decision("<|tool_call_start|>read_file(path='b.rs')<|tool_call_end|>").unwrap();
        match d {
            FinishDecision::ToolCall { name, .. } => assert_eq!(name, "read_file"),
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_invoke_xml() {
        let d = parse_decision(
            "<invoke name=\"read_file\"><parameter name=\"path\">src/main.rs</parameter></invoke>",
        )
        .unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "read_file");
                assert_eq!(arguments["path"], "src/main.rs");
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn parses_malformed_invoke_with_attr_value() {
        let d = parse_decision(
            "<dots_function_call>\n<invoke name=\"run_command\">\n\
             <parameter namecommand=\"rustc test_dbg.rs -o test_dbg && ./test_dbg\" path=\".\" timeout_secs=30>\n\
             </parameter>\n</invoke>",
        )
        .unwrap();
        match d {
            FinishDecision::ToolCall { name, arguments } => {
                assert_eq!(name, "run_command");
                assert_eq!(
                    arguments["command"],
                    "rustc test_dbg.rs -o test_dbg && ./test_dbg"
                );
            }
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn clean_answer_strips_safety_lines() {
        assert_eq!(clean_answer("User Safety: safe\nResponse Safety: safe"), "");
        assert_eq!(
            clean_answer("Here is the answer.\nResponse Safety: safe"),
            "Here is the answer."
        );
        assert_eq!(clean_answer("just normal text"), "just normal text");
    }
}
