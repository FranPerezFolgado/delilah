//! Atlassian Document Format (Jira Cloud descriptions) to plain text with light
//! Markdown, for showing a ticket and handing it to an agent.

use serde_json::Value;

pub fn adf_to_text(doc: &Value) -> String {
    let mut out = String::new();
    blocks(doc, &mut out, 0);
    out.trim().to_string()
}

fn blocks(node: &Value, out: &mut String, depth: usize) {
    for child in children(node) {
        block(child, out, depth);
    }
}

fn block(node: &Value, out: &mut String, depth: usize) {
    match node["type"].as_str().unwrap_or("") {
        "paragraph" => {
            inline(node, out);
            out.push_str("\n\n");
        }
        "heading" => {
            let level = node["attrs"]["level"].as_u64().unwrap_or(2).clamp(1, 6) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            inline(node, out);
            out.push_str("\n\n");
        }
        "bulletList" | "orderedList" => {
            let ordered = node["type"] == "orderedList";
            for (i, item) in children(node).iter().enumerate() {
                out.push_str(&"  ".repeat(depth));
                out.push_str(&if ordered {
                    format!("{}. ", i + 1)
                } else {
                    "- ".into()
                });
                let mut text = String::new();
                for part in children(item) {
                    match part["type"].as_str() {
                        Some("bulletList" | "orderedList") => {
                            text.push('\n');
                            block(part, &mut text, depth + 1);
                        }
                        _ => inline(part, &mut text),
                    }
                }
                out.push_str(text.trim_end());
                out.push('\n');
            }
            if depth == 0 {
                out.push('\n');
            }
        }
        "codeBlock" => {
            out.push_str("```\n");
            inline(node, out);
            out.push_str("\n```\n\n");
        }
        "blockquote" | "panel" => {
            let mut inner = String::new();
            blocks(node, &mut inner, depth);
            for line in inner.trim().lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        "rule" => out.push_str("---\n\n"),
        _ => {
            // Tables, media and unknown nodes: keep whatever text they hold.
            let mut inner = String::new();
            inline(node, &mut inner);
            if !inner.trim().is_empty() {
                out.push_str(inner.trim());
                out.push_str("\n\n");
            }
        }
    }
}

fn inline(node: &Value, out: &mut String) {
    for child in children(node) {
        match child["type"].as_str().unwrap_or("") {
            "text" => {
                let text = child["text"].as_str().unwrap_or("");
                let code = child["marks"]
                    .as_array()
                    .is_some_and(|marks| marks.iter().any(|m| m["type"] == "code"));
                if code {
                    out.push('`');
                    out.push_str(text);
                    out.push('`');
                } else {
                    out.push_str(text);
                }
            }
            "hardBreak" => out.push('\n'),
            "mention" => out.push_str(child["attrs"]["text"].as_str().unwrap_or("@someone")),
            "emoji" => out.push_str(child["attrs"]["text"].as_str().unwrap_or("")),
            "inlineCard" => out.push_str(child["attrs"]["url"].as_str().unwrap_or("")),
            _ => inline(child, out),
        }
    }
}

fn children(node: &Value) -> &[Value] {
    node["content"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn renders_paragraphs_headings_lists_and_code() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [
                { "type": "text", "text": "Falla " },
                { "type": "text", "text": "RetryPolicy", "marks": [{ "type": "code" }] },
                { "type": "hardBreak" },
                { "type": "text", "text": "a veces." }
            ]},
            { "type": "heading", "attrs": { "level": 3 }, "content": [{ "type": "text", "text": "Criterios" }] },
            { "type": "orderedList", "content": [
                { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Uno" }] }] },
                { "type": "listItem", "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "Dos" }] },
                    { "type": "bulletList", "content": [
                        { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "dos a" }] }] }
                    ]}
                ]}
            ]},
            { "type": "codeBlock", "content": [{ "type": "text", "text": "curl -X POST /charges" }] }
        ]});
        assert_eq!(
            adf_to_text(&doc),
            "Falla `RetryPolicy`\na veces.\n\n### Criterios\n\n1. Uno\n2. Dos\n  - dos a\n\n```\ncurl -X POST /charges\n```"
        );
    }

    #[test]
    fn keeps_mentions_and_tolerates_empty_or_unknown_nodes() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "paragraph", "content": [
                { "type": "mention", "attrs": { "text": "@Ana" } },
                { "type": "text", "text": " revisa esto" }
            ]},
            { "type": "mediaSingle", "content": [{ "type": "media" }] }
        ]});
        assert_eq!(adf_to_text(&doc), "@Ana revisa esto");
        assert_eq!(adf_to_text(&json!(null)), "");
    }
}
