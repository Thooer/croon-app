//! `model.generate` 的宿主适配：按 `model.provider` 配置的 OpenAI Chat Completions 实现。
//! 参考：docs/Model/README.md（宿主适配）。

mod blocks;

pub fn schema(name: &str) -> serde_json::Value {
    blocks::json(name)
}

use croon_port::host::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RESPONSE_LIMIT: u64 = 8 * 1024 * 1024;
const ERROR_LIMIT: u64 = 16 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct OpenAiCompatible {
    pub identity: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_output_tokens: u64,
    pub timeout: Duration,
    children: Arc<Mutex<BTreeMap<String, Arc<Mutex<Child>>>>>,
}

impl OpenAiCompatible {
    pub fn from_config_file(
        path: impl AsRef<Path>,
        identity: impl Into<String>,
    ) -> Result<Self, HostError> {
        let value: Json = serde_json::from_slice(&fs::read(path).map_err(host)?).map_err(host)?;
        croon_eval::schema::validate(&schema("model.provider"), &value)
            .map_err(|error| HostError(format!("model provider config: {error:?}")))?;
        if value["protocol"] != "openai_chat_completions" {
            return Err(HostError("unsupported model provider protocol".into()));
        }
        let string = |name: &str| {
            value
                .get(name)
                .and_then(Json::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| HostError(format!("model provider config missing {name}")))
        };
        let timeout = value["timeout_seconds"]
            .as_u64()
            .filter(|value| *value > 0)
            .ok_or_else(|| HostError("timeout_seconds must be positive".into()))?;
        let max_output_tokens = value["max_output_tokens"]
            .as_u64()
            .filter(|value| *value > 0)
            .ok_or_else(|| HostError("max_output_tokens must be positive".into()))?;
        let base_url = string("base_url")?;
        if !base_url.starts_with("https://")
            && !base_url.starts_with("http://127.0.0.1")
            && !base_url.starts_with("http://localhost")
        {
            return Err(HostError("model endpoint must use HTTPS".into()));
        }
        let authority = base_url
            .split_once("://")
            .map(|(_, rest)| rest.split('/').next().unwrap_or_default())
            .unwrap_or_default();
        if authority.contains(['@', '\r', '\n', '\0']) || base_url.contains(['\r', '\n', '\0']) {
            return Err(HostError(
                "model endpoint contains invalid authority characters".into(),
            ));
        }
        let api_key = string("api_key")?;
        if api_key.contains(['\r', '\n', '\0']) {
            return Err(HostError("api_key contains a control character".into()));
        }
        Ok(Self {
            identity: identity.into(),
            base_url,
            api_key,
            model: string("model")?,
            max_output_tokens,
            timeout: Duration::from_secs(timeout),
            children: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }
}

struct Pending {
    attempt: String,
    child: Arc<Mutex<Child>>,
    children: Arc<Mutex<BTreeMap<String, Arc<Mutex<Child>>>>>,
    workdir: PathBuf,
    deadline: Instant,
    result: Option<HostDone>,
}

impl Port for OpenAiCompatible {
    fn id(&self) -> &str {
        &self.identity
    }

    fn channel(&self) -> Channel {
        Channel::Discardable
    }

    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        croon_eval::schema::validate(&schema("model.generate")["in"], call.value)
            .map_err(|error| HostError(format!("model.generate input: {error:?}")))?;
        let max_output_tokens = call.value["max_output_tokens"]
            .as_u64()
            .unwrap_or(self.max_output_tokens);
        let timeout = call.value["timeout"]
            .as_u64()
            .map(Duration::from_secs)
            .unwrap_or(self.timeout);
        if timeout.is_zero() {
            return Err(HostError("model.generate timeout must be positive".into()));
        }
        let request = build_request(call.value, &self.model, max_output_tokens);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let workdir =
            std::env::temp_dir().join(format!("croon-model-{}-{serial}", std::process::id()));
        fs::create_dir(&workdir).map_err(host)?;
        let workdir_guard = Workdir(workdir.clone());
        fs::set_permissions(&workdir, fs::Permissions::from_mode(0o700)).map_err(host)?;
        let request_path = workdir.join("request.json");
        let response_path = workdir.join("response.json");
        let error_path = workdir.join("curl.stderr");
        write_private(&request_path, &serde_json::to_vec(&request).map_err(host)?)?;
        let response = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&response_path)
            .map_err(host)?;
        let stderr = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&error_path)
            .map_err(host)?;
        let curl_config = curl_config(
            &format!("{}/chat/completions", self.base_url.trim_end_matches('/')),
            &self.api_key,
            &request_path,
            timeout,
        );
        let mut child = Command::new("curl")
            .args(["--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::from(response))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| {
                let _ = fs::remove_dir_all(&workdir);
                host(error)
            })?;
        let mut stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HostError("curl stdin unavailable".into()));
            }
        };
        if let Err(error) = stdin.write_all(curl_config.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_dir_all(&workdir);
            return Err(host(error));
        }
        let child = Arc::new(Mutex::new(child));
        let mut children = match self.children.lock() {
            Ok(children) => children,
            Err(_) => {
                if let Ok(mut child) = child.lock() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                return Err(HostError("model child registry poisoned".into()));
            }
        };
        if children.contains_key(call.attempt) {
            drop(children);
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            return Err(HostError("duplicate model attempt".into()));
        }
        children.insert(call.attempt.into(), child.clone());
        drop(children);
        workdir_guard.disarm();
        Ok(Box::new(Pending {
            attempt: call.attempt.into(),
            child,
            children: self.children.clone(),
            workdir,
            deadline: Instant::now() + timeout,
            result: None,
        }))
    }

    fn retire(&self, attempt: &str) -> Result<(), HostError> {
        let child = self
            .children
            .lock()
            .map_err(|_| HostError("model child registry poisoned".into()))?
            .remove(attempt);
        if let Some(child) = child {
            let mut child = child
                .lock()
                .map_err(|_| HostError("model child lock poisoned".into()))?;
            if child.try_wait().map_err(host)?.is_none() {
                child.kill().map_err(host)?;
                child.wait().map_err(host)?;
            }
        }
        Ok(())
    }
}

impl Execution for Pending {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        if let Some(result) = &self.result {
            return Ok(Some(result.clone()));
        }
        let status = {
            let mut child = self
                .child
                .lock()
                .map_err(|_| HostError("model child lock poisoned".into()))?;
            match child.try_wait().map_err(host)? {
                Some(status) => status,
                None if Instant::now() < self.deadline => return Ok(None),
                None => {
                    child.kill().map_err(host)?;
                    child.wait().map_err(host)?
                }
            }
        };
        self.children
            .lock()
            .map_err(|_| HostError("model child registry poisoned".into()))?
            .remove(&self.attempt);
        if !status.success() {
            let diagnostic = read_bounded(&self.workdir.join("curl.stderr"), ERROR_LIMIT)?;
            return Err(HostError(format!(
                "model HTTP request failed ({}): {}",
                status,
                String::from_utf8_lossy(&diagnostic)
            )));
        }
        let bytes = read_bounded(&self.workdir.join("response.json"), RESPONSE_LIMIT)?;
        let response: Json = serde_json::from_slice(&bytes)
            .map_err(|error| HostError(format!("model response JSON: {error}")))?;
        let outcome = parse_response(response)?;
        croon_eval::schema::validate(&schema("model.generate")["out"]["value"], &outcome["value"])
            .map_err(|error| HostError(format!("model.generate response: {error:?}")))?;
        let result = HostDone { outcome };
        self.result = Some(result.clone());
        Ok(Some(result))
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(mut children) = self.children.lock() {
            children.remove(&self.attempt);
        }
        let _ = fs::remove_dir_all(&self.workdir);
    }
}

struct Workdir(PathBuf);
impl Workdir {
    fn disarm(self) {
        std::mem::forget(self);
    }
}
impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn build_request(input: &Json, model: &str, max_output_tokens: u64) -> Json {
    let mut messages = vec![json!({"role":"system", "content":input["system"]})];
    for message in input["messages"].as_array().into_iter().flatten() {
        match message["role"].as_str().unwrap_or_default() {
            "user" => messages.push(json!({"role":"user", "content":message["text"]})),
            "assistant" => {
                let mut assistant = json!({"role":"assistant", "content":message["text"]});
                if let Some(calls) = message["calls"]
                    .as_array()
                    .filter(|calls| !calls.is_empty())
                {
                    assistant["tool_calls"] = json!(calls.iter().map(|call| json!({
                        "id":call["id"], "type":"function",
                        "function":{"name":call["name"], "arguments":call["input"].to_string()}
                    })).collect::<Vec<_>>());
                }
                messages.push(assistant);
            }
            "tool" => {
                for result in message["results"].as_array().into_iter().flatten() {
                    messages.push(json!({"role":"tool", "tool_call_id":result["id"], "content":result["output"].to_string()}));
                }
            }
            _ => unreachable!("model.generate schema checked message role"),
        }
    }
    let tools: Vec<_> = input["tools"].as_array().into_iter().flatten().map(|tool| json!({
        "type":"function", "function":{"name":tool["name"], "description":tool["description"], "parameters":tool["input"]}
    })).collect();
    let mut request =
        json!({"model":model,"messages":messages,"max_completion_tokens":max_output_tokens});
    if !tools.is_empty() {
        request["tools"] = json!(tools);
        request["tool_choice"] = json!("auto");
    }
    request
}

fn parse_response(payload: Json) -> Result<Json, HostError> {
    let choice = payload["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .ok_or_else(|| HostError("completion response has no choices".into()))?;
    let message = &choice["message"];
    let text = if let Some(text) = message["content"].as_str() {
        text.to_owned()
    } else {
        message["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| part["text"].as_str())
            .collect::<String>()
    };
    let calls: Vec<Json> = message["tool_calls"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|call| {
            let arguments = call["function"]["arguments"]
                .as_str()
                .ok_or_else(|| HostError("tool call arguments must be JSON text".into()))?;
            let input: Json = serde_json::from_str(arguments)
                .map_err(|error| HostError(format!("tool call arguments: {error}")))?;
            Ok(json!({"id":call["id"], "name":call["function"]["name"], "input":input}))
        })
        .collect::<Result<_, HostError>>()?;
    let finish = match choice["finish_reason"].as_str().unwrap_or("other") {
        "stop" => "stop",
        "length" => "length",
        "tool_calls" | "function_call" => "tool_calls",
        "content_filter" => "content_filter",
        _ => "other",
    };
    let usage = &payload["usage"];
    Ok(json!({"ok":true,"value":{
        "text":text,"reasoning":Json::Null,"calls":calls,"finish":finish,
        "usage":{"input":usage["prompt_tokens"],"output":usage["completion_tokens"]}
    },"dependencies":[]}))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), HostError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(host)?;
    file.write_all(bytes).map_err(host)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, HostError> {
    let metadata = fs::symlink_metadata(path).map_err(host)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(HostError(
            "model response exceeds the configured adapter limit".into(),
        ));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(host)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1).read_to_end(&mut bytes).map_err(host)?;
    if bytes.len() as u64 > limit {
        return Err(HostError(
            "model response exceeds the configured adapter limit".into(),
        ));
    }
    Ok(bytes)
}

fn curl_config(url: &str, key: &str, request_path: &Path, timeout: Duration) -> String {
    let quote = |value: &str| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""));
    format!(
        "url = {}\nrequest = \"POST\"\nheader = \"content-type: application/json\"\nheader = {}\ndata-binary = {}\nmax-filesize = {}\nmax-time = {}\nsilent\nshow-error\nfail-with-body\n",
        quote(url),
        quote(&format!("Authorization: Bearer {key}")),
        quote(&format!("@{}", request_path.display())),
        RESPONSE_LIMIT,
        timeout.as_secs().max(1)
    )
}

fn host(error: impl std::fmt::Display) -> HostError {
    HostError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use croon_port::store::{Budgets, Content, ContentStore, Scope};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::rc::Rc;
    use std::thread;

    fn fixture(response: Json, delay: Duration) -> (String, thread::JoinHandle<(Json, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let headers = String::from_utf8(request).unwrap();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let body: Json = serde_json::from_slice(&body).unwrap();
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            let response = serde_json::to_vec(&response).unwrap();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                response.len()
            );
            let _ = stream.write_all(&response);
            (body, headers)
        });
        (format!("http://{address}"), server)
    }

    fn config(base_url: String, timeout: Duration) -> OpenAiCompatible {
        OpenAiCompatible {
            identity: "fixture-model".into(),
            base_url,
            api_key: "fixture-secret".into(),
            model: "fixture-model".into(),
            max_output_tokens: 256,
            timeout,
            children: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn call<'a>(content: Rc<Content>, value: &'a Json, attempt: &'a str) -> Call<'a> {
        Call {
            instance: "instance",
            generation: 1,
            key: "0",
            source: "host",
            attempt,
            value,
            dependencies: &[],
            content,
        }
    }

    fn content() -> Rc<ContentStore> {
        let path = std::env::temp_dir().join(format!(
            "model-fixture-content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Rc::new(ContentStore::open(path).unwrap())
    }

    fn content_scope(store: Rc<ContentStore>) -> Rc<Content> {
        Rc::new(
            Content::new(
                store,
                Scope::default(),
                Budgets {
                    nodes: 10,
                    read_bytes: 1024,
                    write_bytes: 1024,
                },
            )
            .unwrap(),
        )
    }

    #[test]
    fn maps_library_tool_exchange_to_and_from_chat_completions() {
        let response = json!({"choices":[{"finish_reason":"tool_calls","message":{"content":"","tool_calls":[{"id":"call-2","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"main.py\"}"}}]}}],"usage":{"prompt_tokens":21,"completion_tokens":4}});
        let (base_url, server) = fixture(response, Duration::ZERO);
        let adapter = config(base_url, Duration::from_secs(5));
        let input = json!({
            "system":"system text",
            "messages":[
                {"role":"user","text":"inspect"},
                {"role":"assistant","text":"","reasoning":null,"calls":[{"id":"call-1","name":"read_file","input":{"path":"main.py"}}]},
                {"role":"tool","results":[{"id":"call-1","name":"read_file","output":"contents","is_error":false}]}
            ],
            "tools":[{"name":"read_file","description":"read","input":{"type":"object"}}],
            "max_output_tokens":37,"timeout":9
        });
        let mut execution = adapter
            .start(&call(content_scope(content()), &input, "attempt-tool"))
            .unwrap();
        let output = loop {
            if let Some(output) = execution.collect().unwrap() {
                break output;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let (request, headers) = server.join().unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-secret")
        );
        assert_eq!(request["model"], "fixture-model");
        assert_eq!(request["max_completion_tokens"], 37);
        assert_eq!(
            request["messages"][0],
            json!({"role":"system","content":"system text"})
        );
        assert_eq!(
            request["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"main.py\"}"
        );
        assert_eq!(request["messages"][3]["tool_call_id"], "call-1");
        assert_eq!(
            request["tools"][0]["function"]["parameters"]["type"],
            "object"
        );
        assert_eq!(
            output.outcome["value"]["calls"][0],
            json!({"id":"call-2","name":"read_file","input":{"path":"main.py"}})
        );
        assert_eq!(
            output.outcome["value"]["usage"],
            json!({"input":21,"output":4})
        );
    }

    #[test]
    fn config_file_is_validated_and_selects_the_declared_protocol() {
        let root = std::env::temp_dir().join(format!(
            "model-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("provider.json");
        fs::write(&path, json!({"protocol":"openai_chat_completions","base_url":"https://provider.invalid/v1","api_key":"local-test-key","model":"fixture-model","max_output_tokens":128,"timeout_seconds":7}).to_string()).unwrap();
        let config = OpenAiCompatible::from_config_file(&path, "fixture").unwrap();
        assert_eq!(config.id(), "fixture");
        assert_eq!(config.model, "fixture-model");
        assert_eq!(config.timeout, Duration::from_secs(7));
        fs::write(&path, json!({"protocol":"unknown"}).to_string()).unwrap();
        assert!(OpenAiCompatible::from_config_file(&path, "fixture").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_tool_arguments_and_http_deadlines_are_errors() {
        let error = parse_response(json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[{"function":{"arguments":"{"}}]}}]})).unwrap_err();
        assert!(error.0.contains("tool call arguments"));
        let (base_url, server) = fixture(
            json!({"choices":[{"finish_reason":"stop","message":{"content":"late"}}]}),
            Duration::from_millis(150),
        );
        let adapter = config(base_url, Duration::from_millis(20));
        let input = json!({"system":"s","messages":[],"tools":[]});
        let mut execution = adapter
            .start(&call(content_scope(content()), &input, "attempt-timeout"))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match execution.collect() {
                Err(error) => {
                    assert!(error.0.contains("model HTTP request failed"));
                    break;
                }
                Ok(None) => {
                    assert!(
                        Instant::now() < deadline,
                        "request deadline was not enforced"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(Some(_)) => panic!("late response unexpectedly succeeded"),
            }
        }
        let _ = server.join();
    }
}
