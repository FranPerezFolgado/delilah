//! Pieces shared by the headless, one-shot Claude runs (submodule suggestions, branch names).

use std::ffi::OsString;

use serde_json::Value;

/// The same clean environment as the interactive agent.
pub fn env() -> Vec<(String, OsString)> {
    let mut vars = crate::env::agent_env();
    vars.push(("TERM".into(), "dumb".into()));
    vars
}

/// Reads Claude's `--output-format json` envelope and returns the JSON object the model
/// answered with (the text between its first `{` and last `}`).
pub fn answer_json(output: &str) -> Result<Value, String> {
    let envelope: Value =
        serde_json::from_str(output).map_err(|e| format!("unexpected output from claude: {e}"))?;
    if envelope["is_error"] == true {
        return Err(format!(
            "claude failed: {}",
            envelope["result"].as_str().unwrap_or("unknown error")
        ));
    }
    let text = envelope["result"].as_str().ok_or("claude gave no result")?;
    let (start, end) = (text.find('{'), text.rfind('}'));
    let (Some(start), Some(end)) = (start, end) else {
        return Err(format!(
            "claude did not answer with JSON: {}",
            text.chars().take(200).collect::<String>()
        ));
    };
    serde_json::from_str(&text[start..=end]).map_err(|e| format!("claude's JSON is not valid: {e}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn finds_the_object_inside_prose() {
        let out = json!({ "is_error": false, "result": "Sí: {\"a\": 1} listo" }).to_string();
        assert_eq!(answer_json(&out).unwrap()["a"], 1);
    }

    #[test]
    fn reports_each_failure() {
        assert!(answer_json("nope").unwrap_err().contains("unexpected"));
        let failed = json!({ "is_error": true, "result": "limit" }).to_string();
        assert!(answer_json(&failed).unwrap_err().contains("limit"));
        let none = json!({ "is_error": false }).to_string();
        assert!(answer_json(&none).unwrap_err().contains("no result"));
        let prose = json!({ "is_error": false, "result": "hola" }).to_string();
        assert!(answer_json(&prose).unwrap_err().contains("JSON"));
        let bad = json!({ "is_error": false, "result": "{x}" }).to_string();
        assert!(answer_json(&bad).unwrap_err().contains("not valid"));
    }
}
