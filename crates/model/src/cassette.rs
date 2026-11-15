//! 录制与回放：`model.generate` 的另外两种宿主实现，形状是 `model.cassette`。
//! 参考：docs/Model/README.md（录制与回放）。

use crate::schema;
use croon_port::host::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn host(error: impl std::fmt::Display) -> HostError {
    HostError(error.to_string())
}

fn check_request(value: &Json) -> Result<String, HostError> {
    croon_eval::schema::validate(&schema("model.generate")["in"], value)
        .map_err(|error| HostError(format!("model.generate input: {error:?}")))?;
    croon_eval::value::canonical(value).map_err(host)
}

/// 包住一个 `model.generate` 的 Port，把每次完成的请求与结果追加进录像；恢复身份与通道沿用被包住的那个。
pub struct Recorder {
    inner: Rc<dyn Port>,
    tape: Rc<Tape>,
}

struct Tape {
    path: PathBuf,
    entries: RefCell<Vec<Json>>,
}

impl Tape {
    fn push(&self, request: Json, outcome: Json) -> Result<(), HostError> {
        self.entries
            .borrow_mut()
            .push(json!({"request": request, "outcome": outcome}));
        let text =
            serde_json::to_vec_pretty(&json!({"entries": *self.entries.borrow()})).map_err(host)?;
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, text).map_err(host)?;
        fs::rename(&temporary, &self.path).map_err(host)
    }
}

impl Recorder {
    /// 录像文件不能已经存在：一次录制一盘，不在旧录像上追加。
    pub fn new(inner: Rc<dyn Port>, path: impl Into<PathBuf>) -> Result<Self, HostError> {
        let path = path.into();
        if fs::symlink_metadata(&path).is_ok() {
            return Err(HostError(format!("录像已存在：{}", path.display())));
        }
        Ok(Self {
            inner,
            tape: Rc::new(Tape {
                path,
                entries: RefCell::new(Vec::new()),
            }),
        })
    }
}

impl Port for Recorder {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn channel(&self) -> Channel {
        self.inner.channel()
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        check_request(call.value)?;
        Ok(Box::new(Recording {
            inner: Some(self.inner.start(call)?),
            request: call.value.clone(),
            tape: self.tape.clone(),
            done: None,
        }))
    }
    fn retire(&self, attempt: &str) -> Result<(), HostError> {
        self.inner.retire(attempt)
    }
}

struct Recording {
    inner: Option<Box<dyn Execution>>,
    request: Json,
    tape: Rc<Tape>,
    done: Option<HostDone>,
}

impl Execution for Recording {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        if let Some(done) = &self.done {
            return Ok(Some(done.clone()));
        }
        let Some(done) = self.inner.as_mut().expect("未丢弃").collect()? else {
            return Ok(None);
        };
        self.tape.push(self.request.clone(), done.outcome.clone())?;
        self.done = Some(done.clone());
        Ok(Some(done))
    }
    fn discard(mut self: Box<Self>) {
        if let Some(inner) = self.inner.take() {
            inner.discard();
        }
    }
}

/// 只读一盘录像，按请求的规范文本给出录下的结果，不连任何 Provider。
pub struct Replayer {
    identity: String,
    state: RefCell<Replay>,
}

struct Replay {
    /// 规范文本 → 按录制先后的结果。
    queues: BTreeMap<String, Vec<Json>>,
    /// 规范文本 → 已领走几条。
    used: BTreeMap<String, usize>,
    /// 逻辑行动（Instance、键）→ 它领到的结果；重做拿同一条。
    assigned: BTreeMap<(String, String), Json>,
}

impl Replayer {
    pub fn open(path: impl AsRef<Path>, identity: impl Into<String>) -> Result<Self, HostError> {
        let value: Json = serde_json::from_slice(&fs::read(path).map_err(host)?).map_err(host)?;
        croon_eval::schema::validate(&schema("model.cassette"), &value)
            .map_err(|error| HostError(format!("model.cassette: {error:?}")))?;
        let mut queues: BTreeMap<String, Vec<Json>> = BTreeMap::new();
        for entry in value["entries"].as_array().into_iter().flatten() {
            queues
                .entry(croon_eval::value::canonical(&entry["request"]).map_err(host)?)
                .or_default()
                .push(entry["outcome"].clone());
        }
        Ok(Self {
            identity: identity.into(),
            state: RefCell::new(Replay {
                queues,
                used: BTreeMap::new(),
                assigned: BTreeMap::new(),
            }),
        })
    }
}

impl Port for Replayer {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        let text = check_request(call.value)?;
        let action = (call.instance.to_owned(), call.key.to_owned());
        let mut state = self.state.borrow_mut();
        if let Some(outcome) = state.assigned.get(&action) {
            return Ok(Box::new(Ready(HostDone {
                outcome: outcome.clone(),
            })));
        }
        let Some(queue) = state.queues.get(&text) else {
            return Err(HostError("录像里没有这个请求；请求变了就重录".into()));
        };
        let n = state.used.get(&text).copied().unwrap_or(0);
        let Some(outcome) = queue.get(n).cloned() else {
            return Err(HostError(format!("这个请求在录像里只有 {n} 条，已经用尽")));
        };
        state.used.insert(text, n + 1);
        state.assigned.insert(action, outcome.clone());
        Ok(Box::new(Ready(HostDone { outcome })))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

struct Ready(HostDone);

impl Execution for Ready {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        Ok(Some(self.0.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{config, content, content_scope, fixture};
    use croon_port::store::Content;
    use std::time::Duration;

    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "croon-cassette-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn call<'a>(content: Rc<Content>, value: &'a Json, key: &'a str) -> Call<'a> {
        Call {
            instance: "instance",
            generation: 1,
            key,
            source: "host",
            attempt: key,
            value,
            dependencies: &[],
            content,
        }
    }

    fn finish(mut execution: Box<dyn Execution>) -> HostDone {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(done) = execution.collect().unwrap() {
                return done;
            }
            assert!(std::time::Instant::now() < deadline, "collection deadline");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_recording_replays_by_logical_action_and_holds_no_credentials() {
        let response = json!({"choices":[{"finish_reason":"stop","message":{"content":"done"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}});
        let (base_url, server) = fixture(response, Duration::ZERO);
        let root = temp();
        let path = root.join("cassette.json");
        let recorder =
            Recorder::new(Rc::new(config(base_url, Duration::from_secs(5))), &path).unwrap();
        let input = json!({"system":"s","messages":[{"role":"user","text":"hi"}],"tools":[]});
        let recorded = finish(
            recorder
                .start(&call(content_scope(content()), &input, "0"))
                .unwrap(),
        );
        server.join().unwrap();
        assert!(
            !fs::read_to_string(&path)
                .unwrap()
                .contains("fixture-secret"),
            "录像不含凭据"
        );

        let replayer = Replayer::open(&path, recorder.id()).unwrap();
        assert_eq!(replayer.id(), recorder.id());
        let replayed = finish(
            replayer
                .start(&call(content_scope(content()), &input, "0"))
                .unwrap(),
        );
        assert_eq!(replayed.outcome, recorded.outcome);

        // 同一逻辑行动重做（新 attempt、同一个键）拿同一条，不多领。
        let redone = finish(
            replayer
                .start(&call(content_scope(content()), &input, "0"))
                .unwrap(),
        );
        assert_eq!(redone.outcome, recorded.outcome);

        // 另一个逻辑行动发同样的请求：录像里只有一条，已经被领走。
        let exhausted = replayer
            .start(&call(content_scope(content()), &input, "1"))
            .err()
            .expect("用尽要报宿主故障");
        assert!(exhausted.0.contains("用尽"), "{exhausted}");

        // 请求改一个字就对不上，不改去调真模型。
        let changed = json!({"system":"s","messages":[{"role":"user","text":"hi!"}],"tools":[]});
        let missing = replayer
            .start(&call(content_scope(content()), &changed, "2"))
            .err()
            .expect("对不上要报宿主故障");
        assert!(missing.0.contains("没有这个请求"), "{missing}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_existing_cassette_is_never_overwritten() {
        let root = temp();
        let path = root.join("cassette.json");
        fs::write(&path, "{\"entries\":[]}").unwrap();
        let inner = Rc::new(config(
            "https://provider.invalid".into(),
            Duration::from_secs(1),
        ));
        assert!(Recorder::new(inner, &path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"entries\":[]}");
        fs::remove_dir_all(root).unwrap();
    }
}
