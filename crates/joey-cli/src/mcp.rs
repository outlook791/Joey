//! `joey --mcp`: Joey as a Model Context Protocol server, so any AI app
//! that speaks MCP (local models included) can see what a file is and
//! convert it. JSON-RPC 2.0, one message per line on stdin/stdout.
//!
//! The AI gets the same safety as a person: results go next to the
//! originals (or in a chosen folder), nothing is overwritten or deleted.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use joey_core::diag;
use joey_core::options::parse_time;
use joey_core::{Format, Joey, Options, Quality, Tool, ToolParams};
use serde_json::{json, Value};

const PROTOCOL: &str = "2025-06-18";

pub fn serve() -> std::io::Result<()> {
    let joey = Joey::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&joey, &msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The answer to one message; notifications get none.
fn handle(joey: &Joey, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "joey", "title": "Joey", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Joey converts files on this computer with the tools installed here. \
                    Call joey_inspect first to see what each file is and what it can become, \
                    then joey_convert or joey_tool. Results are written next to the originals \
                    (or in out_dir); nothing is overwritten or deleted."
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_list()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(joey, name, &args) {
                Ok(v) => json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}],
                    "structuredContent": v,
                    "isError": false
                }),
                Err(m) => json!({
                    "content": [{"type": "text", "text": m}],
                    "isError": true
                }),
            }
        }
        _ => return Some(error(id, -32601, &format!("unknown method {method}"))),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tool_list() -> Value {
    let formats: Vec<&str> = Format::ALL.iter().map(|f| f.extension()).collect();
    let tools: Vec<&str> = Tool::ALL.iter().map(|t| t.key()).collect();
    let paths = json!({
        "type": "array", "items": {"type": "string"}, "minItems": 1,
        "description": "Absolute paths of the files"
    });
    json!([
        {
            "name": "joey_inspect",
            "title": "What can these files become?",
            "description": "Recognises each file by its content and lists the formats it can be converted to and the quick tools that apply, on this computer.",
            "inputSchema": {"type": "object", "properties": {"paths": paths}, "required": ["paths"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "joey_convert",
            "title": "Convert files",
            "description": "Converts files to another format. Writes new files next to the originals (or in out_dir) and never overwrites. Settings are optional.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paths": paths,
                    "to": {"type": "string", "enum": formats, "description": "Target format"},
                    "out_dir": {"type": "string", "description": "Folder for the results (default: next to each file)"},
                    "quality": {"type": "string", "enum": ["low", "medium", "high", "max"]},
                    "max_size": {"type": "integer", "description": "Longest side in pixels, for images and video"},
                    "strip_metadata": {"type": "boolean", "description": "Remove camera, location and author details"},
                    "no_audio": {"type": "boolean", "description": "Video without sound"},
                    "pages": {"type": "string", "description": "PDF pages, e.g. \"1-3,7\""},
                    "dpi": {"type": "integer", "description": "Resolution for PDF pages to images"}
                },
                "required": ["paths", "to"]
            },
            "annotations": {"destructiveHint": false, "idempotentHint": false}
        },
        {
            "name": "joey_tool",
            "title": "Quick tool",
            "description": "Runs a quick tool: compress, rotate, crop, trim, merge or split PDFs, protect or unlock PDFs, join audio or video, extract archives, zip. Merging tools take all paths at once.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": {"type": "string", "enum": tools},
                    "paths": paths,
                    "start": {"type": "string", "description": "trim: start time, e.g. 0:30"},
                    "end": {"type": "string", "description": "trim: end time, e.g. 1:45"},
                    "pages": {"type": "string", "description": "extract-pages: e.g. \"1-3,7\""},
                    "shape": {"type": "string", "description": "crop: width:height, e.g. 1:1 or 16:9"},
                    "password": {"type": "string", "description": "protect-pdf / unlock-pdf"}
                },
                "required": ["tool", "paths"]
            },
            "annotations": {"destructiveHint": false}
        },
        {
            "name": "joey_engines",
            "title": "Installed conversion tools",
            "description": "Lists the conversion programs Joey found on this computer.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true}
        }
    ])
}

fn paths(args: &Value) -> Result<Vec<PathBuf>, String> {
    let list = args
        .get("paths")
        .and_then(Value::as_array)
        .ok_or("paths is required")?;
    let v: Vec<PathBuf> = list
        .iter()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    if v.is_empty() {
        return Err("paths is empty".into());
    }
    Ok(v)
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

/// One file's outcome, with the error code people and AIs can act on.
fn outcome(path: &std::path::Path, o: &joey_core::Outcome) -> Value {
    match o {
        Ok(files) => json!({"path": path, "ok": true, "outputs": files}),
        Err(e) => {
            let c = diag::classify(e, &[]);
            json!({
                "path": path, "ok": false,
                "error": {"code": c.id, "title": c.title, "hint": c.hint, "detail": e.to_string()}
            })
        }
    }
}

fn call(joey: &Joey, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "joey_engines" => Ok(json!({"engines": joey.engine_names()})),
        "joey_inspect" => {
            let files = paths(args)?;
            let list: Vec<Value> = files
                .iter()
                .map(|p| match joey.detect(p) {
                    Ok(f) => json!({
                        "path": p,
                        "format": f.extension(),
                        "kind": format!("{:?}", f.category()).to_lowercase(),
                        "convert_to": joey.targets(f).iter().map(|t| t.extension()).collect::<Vec<_>>(),
                        "tools": joey.tools_for(f, files.len()).iter().map(|t| t.key()).collect::<Vec<_>>()
                    }),
                    Err(e) => outcome(p, &Err(e)),
                })
                .collect();
            Ok(json!({"files": list}))
        }
        "joey_convert" => {
            let files = paths(args)?;
            let to = str_arg(args, "to").ok_or("to is required")?;
            let to = Format::parse(to).ok_or_else(|| format!("unknown format {to}"))?;
            let mut o = Options::default();
            if let Some(q) = str_arg(args, "quality") {
                o.quality = Quality::from_key(q).ok_or_else(|| format!("unknown quality {q}"))?;
            }
            o.max_size = args
                .get("max_size")
                .and_then(Value::as_u64)
                .map(|x| x as u32);
            o.strip_metadata = args.get("strip_metadata").and_then(Value::as_bool) == Some(true);
            o.no_audio = args.get("no_audio").and_then(Value::as_bool) == Some(true);
            o.pages = str_arg(args, "pages").map(str::to_string);
            o.dpi = args.get("dpi").and_then(Value::as_u64).map(|x| x as u32);
            let out_dir = str_arg(args, "out_dir").map(PathBuf::from);
            let results = joey.convert_all(&files, to, out_dir.as_deref(), &o);
            let list: Vec<Value> = files
                .iter()
                .zip(&results)
                .map(|(p, o)| outcome(p, o))
                .collect();
            Ok(json!({"results": list}))
        }
        "joey_tool" => {
            let files = paths(args)?;
            let key = str_arg(args, "tool").ok_or("tool is required")?;
            let tool = Tool::from_key(key).ok_or_else(|| format!("unknown tool {key}"))?;
            let mut p = ToolParams::default();
            if let (Some(a), Some(b)) = (str_arg(args, "start"), str_arg(args, "end")) {
                let a = parse_time(a).ok_or("start: use a time like 0:30")?;
                let b = parse_time(b).ok_or("end: use a time like 1:45")?;
                p.trim = Some((a, b));
            }
            p.pages = str_arg(args, "pages").map(str::to_string);
            if let Some(s) = str_arg(args, "shape") {
                let (w, h) = s
                    .split_once(':')
                    .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
                    .ok_or("shape: use width:height, e.g. 16:9")?;
                p.shape = Some((w, h));
            }
            p.password = str_arg(args, "password").map(str::to_string);
            let results = joey.run_tool_with(tool, &files, &p);
            let list: Vec<Value> = if tool.merges() {
                results.iter().map(|o| outcome(&files[0], o)).collect()
            } else {
                files
                    .iter()
                    .zip(&results)
                    .map(|(p, o)| outcome(p, o))
                    .collect()
            };
            Ok(json!({"results": list}))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_tools_and_answers_errors() {
        let joey = Joey::with_engines(Vec::new());
        let init = handle(
            &joey,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}),
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(handle(
            &joey,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .is_none());
        let list = handle(
            &joey,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 4);
        let missing = handle(
            &joey,
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"joey_inspect","arguments":{"paths":["/nope/x.jpg"]}}}),
        )
        .unwrap();
        let f = &missing["result"]["structuredContent"]["files"][0];
        assert_eq!(f["error"]["code"], "E101");
    }
}
