//! OpenAI Realtime 轉錄協定（純函式，不碰網路）。

use std::collections::VecDeque;

use base64::Engine as _;
use serde_json::{json, Value};

/// 轉錄專用的 Realtime 端點。
pub const REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";
/// 本機管線交給工作階段的音訊取樣率（重採樣後）。
pub const SAMPLE_RATE: u32 = 16_000;
/// 實際送給 OpenAI 的取樣率：`audio/pcm` 只接受 ≥ 24 kHz（實測 16 kHz 會被拒絕）。
pub const WIRE_SAMPLE_RATE: u32 = 24_000;
/// 每個 `input_audio_buffer.append` 封包的樣本數（100 ms）。
pub const FRAME_SAMPLES: usize = 1_600;
/// 伺服器接受 commit 的最短音訊長度（100 ms）；不足時補靜音。
// 略多於 100 ms：升頻到 24 kHz 時串流插值可能少 1 個樣本，避免剛好低於伺服器下限。
pub const MIN_COMMIT_SAMPLES: usize = 1_700;

/// 一次轉錄工作階段的設定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    pub model: String,
    /// ISO-639-1 語言碼；`None` = 自動偵測。
    pub language: Option<String>,
    /// 情境提示詞（模板）。
    pub prompt: Option<String>,
}

impl SessionConfig {
    /// 由 whisper 的語言 / 提示詞設定推導（`"auto"` 或空字串 → 自動偵測；`"zh-TW"` → `"zh"`）。
    pub fn from_whisper(model: &str, language: Option<&str>, prompt: Option<&str>) -> Self {
        let language = language
            .map(|l| {
                l.trim()
                    .split(['-', '_'])
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
            })
            .filter(|l| !l.is_empty() && l != "auto");
        let prompt = prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        Self {
            model: model.trim().to_string(),
            language,
            prompt,
        }
    }
}

/// `session.update`：轉錄模式、PCM16 24 kHz、停用伺服器端 VAD（由本機 VAD 決定 commit）。
pub fn session_update(cfg: &SessionConfig) -> String {
    let mut transcription = json!({ "model": cfg.model });
    // 只有 gpt-live-transcribe 支援 delay；gpt-transcribe 收到會回錯誤（實測）。
    if cfg.model == "gpt-live-transcribe" {
        transcription["delay"] = json!("low");
    }
    if let Some(lang) = &cfg.language {
        transcription["languages"] = json!([lang]);
    }
    if let Some(prompt) = &cfg.prompt {
        transcription["prompt"] = json!(prompt);
    }
    json!({
        "type": "session.update",
        "session": {
            "type": "transcription",
            "audio": {
                "input": {
                    "format": { "type": "audio/pcm", "rate": WIRE_SAMPLE_RATE },
                    "transcription": transcription,
                    "turn_detection": null
                }
            }
        }
    })
    .to_string()
}

/// 16 kHz → 24 kHz 串流線性插值（3:2），跨封包保留狀態，避免封包邊界的不連續。
#[derive(Debug, Default, Clone)]
pub struct Upsampler {
    prev: Option<f32>,
    /// 下一個輸出點在本批輸入中的位置（-1 代表上一批的最後一個樣本）。
    pos: f64,
}

impl Upsampler {
    const STEP: f64 = SAMPLE_RATE as f64 / WIRE_SAMPLE_RATE as f64;

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        let prev = *self.prev.get_or_insert(input[0]);
        let get = |i: isize| if i < 0 { prev } else { input[i as usize] };
        let last = (input.len() - 1) as f64;
        let mut out = Vec::with_capacity(input.len() * 3 / 2 + 2);
        while self.pos <= last {
            let i = self.pos.floor() as isize;
            let frac = (self.pos - i as f64) as f32;
            let a = get(i);
            let b = if ((i + 1) as usize) < input.len() {
                get(i + 1)
            } else {
                a
            };
            out.push(a + (b - a) * frac);
            self.pos += Self::STEP;
        }
        self.pos -= input.len() as f64;
        self.prev = input.last().copied();
        out
    }
}

/// f32（-1.0–1.0）→ PCM16 little-endian。
pub fn pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let s = if s.is_finite() { s } else { 0.0 };
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// `input_audio_buffer.append`（base64 PCM16）。
pub fn append_event(samples: &[f32]) -> String {
    let audio = base64::engine::general_purpose::STANDARD.encode(pcm16_le(samples));
    json!({ "type": "input_audio_buffer.append", "audio": audio }).to_string()
}

/// `input_audio_buffer.commit`：把目前緩衝的音訊結成一段要轉錄的語句。
pub fn commit_event() -> String {
    json!({ "type": "input_audio_buffer.commit" }).to_string()
}

/// 我們關心的伺服器事件；其餘一律 [`ServerEvent::Other`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    SessionUpdated,
    Committed { item_id: String },
    Delta { item_id: String, delta: String },
    Completed { item_id: String, transcript: String },
    Failed { item_id: String, message: String },
    Error { message: String },
    Other,
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn error_message(v: &Value) -> Option<String> {
    let e = v.get("error")?;
    let msg = e
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| e.as_str())
        .unwrap_or("")
        .trim();
    (!msg.is_empty()).then(|| msg.to_string())
}

/// 解析伺服器文字訊息。不是 JSON 物件或沒有 `type` 時回傳 `None`；未知事件 / 欄位一律容忍。
pub fn parse_server_event(text: &str) -> Option<ServerEvent> {
    let v: Value = serde_json::from_str(text).ok()?;
    let kind = v.get("type")?.as_str()?;
    let item_id = || str_field(&v, "item_id").to_string();
    Some(match kind {
        "session.updated" => ServerEvent::SessionUpdated,
        "input_audio_buffer.committed" => ServerEvent::Committed { item_id: item_id() },
        "conversation.item.input_audio_transcription.delta" => ServerEvent::Delta {
            item_id: item_id(),
            delta: str_field(&v, "delta").to_string(),
        },
        "conversation.item.input_audio_transcription.completed" => ServerEvent::Completed {
            item_id: item_id(),
            transcript: str_field(&v, "transcript").to_string(),
        },
        "conversation.item.input_audio_transcription.failed" => ServerEvent::Failed {
            item_id: item_id(),
            message: error_message(&v).unwrap_or_else(|| "轉錄失敗".into()),
        },
        "error" => ServerEvent::Error {
            message: error_message(&v).unwrap_or_else(|| "未知錯誤".into()),
        },
        _ => ServerEvent::Other,
    })
}

#[derive(Debug)]
struct Item {
    id: String,
    partial: String,
    /// `Some(Some(text))` 已完成；`Some(None)` 失敗；`None` 等待中。
    done: Option<Option<String>>,
}

/// 依 commit 順序釋出完成的語句（後面的語句先完成時先保留，等前面的完成或失敗）。
#[derive(Debug, Default)]
pub struct ReorderBuffer {
    items: VecDeque<Item>,
}

impl ReorderBuffer {
    fn ensure(&mut self, id: &str) -> &mut Item {
        let pos = match self.items.iter().position(|i| i.id == id) {
            Some(pos) => pos,
            None => {
                self.items.push_back(Item {
                    id: id.to_string(),
                    partial: String::new(),
                    done: None,
                });
                self.items.len() - 1
            }
        };
        &mut self.items[pos]
    }

    /// 伺服器確認 commit（決定順序）。
    pub fn committed(&mut self, id: &str) {
        self.ensure(id);
    }

    /// 累積暫時文字。
    pub fn delta(&mut self, id: &str, delta: &str) {
        let item = self.ensure(id);
        if item.done.is_none() {
            item.partial.push_str(delta);
        }
    }

    /// 語句完成；回傳現在可以依序輸出的文字。
    pub fn completed(&mut self, id: &str, transcript: &str) -> Vec<String> {
        self.ensure(id).done = Some(Some(transcript.to_string()));
        self.release()
    }

    /// 語句失敗；回傳因此解除阻擋、可以輸出的文字。
    pub fn failed(&mut self, id: &str) -> Vec<String> {
        self.ensure(id).done = Some(None);
        self.release()
    }

    fn release(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while self.items.front().is_some_and(|i| i.done.is_some()) {
            if let Some(Some(text)) = self.items.pop_front().and_then(|i| i.done) {
                out.push(text);
            }
        }
        out
    }

    /// 尚未輸出的語句目前的暫時文字（依序串接）。
    pub fn partial_text(&self) -> String {
        self.items
            .iter()
            .map(|i| match &i.done {
                Some(Some(text)) => text.as_str(),
                Some(None) => "",
                None => i.partial.as_str(),
            })
            .collect()
    }

    /// 尚未輸出的語句數。
    pub fn outstanding(&self) -> usize {
        self.items.len()
    }

    /// 放棄等待：依序取出已完成的文字，未完成的以暫時文字代替（盡量不丟掉內容）。
    pub fn drain_all(&mut self) -> Vec<String> {
        self.items
            .drain(..)
            .filter_map(|i| match i.done {
                Some(done) => done,
                None => Some(i.partial),
            })
            .filter(|t| !t.trim().is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn session_update_shape() {
        let cfg = SessionConfig::from_whisper("gpt-live-transcribe", Some("zh"), Some("會議"));
        let v = parse(&session_update(&cfg));
        assert_eq!(v["type"], "session.update");
        assert_eq!(v["session"]["type"], "transcription");
        let input = &v["session"]["audio"]["input"];
        assert_eq!(input["format"]["type"], "audio/pcm");
        assert_eq!(input["format"]["rate"], 24_000);
        assert!(input["turn_detection"].is_null());
        assert!(input.as_object().unwrap().contains_key("turn_detection"));
        let tr = &input["transcription"];
        assert_eq!(tr["model"], "gpt-live-transcribe");
        assert_eq!(tr["languages"], json!(["zh"]));
        assert_eq!(tr["prompt"], "會議");
        assert_eq!(tr["delay"], "low");
    }

    #[test]
    fn session_update_omits_auto_language_and_empty_prompt() {
        for lang in [None, Some("auto"), Some(""), Some("  ")] {
            let cfg = SessionConfig::from_whisper("gpt-transcribe", lang, Some("  "));
            assert_eq!(cfg.language, None);
            assert_eq!(cfg.prompt, None);
            let v = parse(&session_update(&cfg));
            let tr = v["session"]["audio"]["input"]["transcription"]
                .as_object()
                .unwrap();
            assert!(!tr.contains_key("languages"));
            assert!(!tr.contains_key("prompt"));
        }
        let cfg = SessionConfig::from_whisper("m", Some("zh-TW"), None);
        assert_eq!(cfg.language.as_deref(), Some("zh"));
    }

    #[test]
    fn pcm16_encoding() {
        let bytes = pcm16_le(&[0.0, 1.0, -1.0, 2.0, f32::NAN, 0.5]);
        let vals: Vec<i16> = bytes
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(vals, vec![0, 32767, -32767, 32767, 0, 16384]);
    }

    #[test]
    fn append_event_is_base64_pcm() {
        let v = parse(&append_event(&[0.0, 1.0]));
        assert_eq!(v["type"], "input_audio_buffer.append");
        let raw = base64::engine::general_purpose::STANDARD
            .decode(v["audio"].as_str().unwrap())
            .unwrap();
        assert_eq!(raw, vec![0, 0, 0xFF, 0x7F]);
        assert_eq!(parse(&commit_event())["type"], "input_audio_buffer.commit");
    }

    #[test]
    fn parses_known_events() {
        assert_eq!(
            parse_server_event(r#"{"type":"session.updated","session":{}}"#),
            Some(ServerEvent::SessionUpdated)
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"input_audio_buffer.committed","item_id":"a","previous_item_id":null}"#
            ),
            Some(ServerEvent::Committed {
                item_id: "a".into()
            })
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"conversation.item.input_audio_transcription.delta","item_id":"a","content_index":0,"delta":"你"}"#
            ),
            Some(ServerEvent::Delta {
                item_id: "a".into(),
                delta: "你".into()
            })
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"你好","usage":{"x":1}}"#
            ),
            Some(ServerEvent::Completed {
                item_id: "a".into(),
                transcript: "你好".into()
            })
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"conversation.item.input_audio_transcription.failed","item_id":"b","error":{"message":"boom"}}"#
            ),
            Some(ServerEvent::Failed {
                item_id: "b".into(),
                message: "boom".into()
            })
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"conversation.item.input_audio_transcription.failed","item_id":"b"}"#
            ),
            Some(ServerEvent::Failed {
                item_id: "b".into(),
                message: "轉錄失敗".into()
            })
        );
        assert_eq!(
            parse_server_event(
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad"}}"#
            ),
            Some(ServerEvent::Error {
                message: "bad".into()
            })
        );
    }

    #[test]
    fn tolerates_unknown_and_garbage() {
        assert_eq!(
            parse_server_event(r#"{"type":"rate_limits.updated","x":[1,2]}"#),
            Some(ServerEvent::Other)
        );
        assert_eq!(parse_server_event("not json"), None);
        assert_eq!(parse_server_event(r#"{"no_type":1}"#), None);
        assert_eq!(parse_server_event("[1,2]"), None);
        // 缺欄位不會 panic
        assert_eq!(
            parse_server_event(r#"{"type":"conversation.item.input_audio_transcription.delta"}"#),
            Some(ServerEvent::Delta {
                item_id: String::new(),
                delta: String::new()
            })
        );
    }

    #[test]
    fn reorder_releases_in_commit_order() {
        let mut b = ReorderBuffer::default();
        b.committed("1");
        b.committed("2");
        b.committed("3");
        b.delta("1", "甲");
        b.delta("2", "乙");
        assert_eq!(b.partial_text(), "甲乙");
        assert!(b.completed("2", "乙完").is_empty());
        assert_eq!(b.partial_text(), "甲乙完");
        assert_eq!(b.completed("1", "甲完"), vec!["甲完", "乙完"]);
        assert_eq!(b.outstanding(), 1);
        assert_eq!(b.failed("3"), Vec::<String>::new());
        assert_eq!(b.outstanding(), 0);
    }

    #[test]
    fn reorder_failure_unblocks_later_items() {
        let mut b = ReorderBuffer::default();
        b.committed("1");
        b.committed("2");
        assert!(b.completed("2", "後").is_empty());
        assert_eq!(b.failed("1"), vec!["後"]);
    }

    #[test]
    fn reorder_unknown_completion_is_appended() {
        let mut b = ReorderBuffer::default();
        b.committed("1");
        assert!(b.completed("x", "x").is_empty());
        assert_eq!(b.completed("1", "一"), vec!["一", "x"]);
    }

    #[test]
    fn reorder_drain_all_keeps_partials() {
        let mut b = ReorderBuffer::default();
        b.committed("1");
        b.committed("2");
        b.committed("3");
        b.delta("1", "半");
        b.completed("2", "完");
        b.failed("3");
        assert_eq!(b.drain_all(), vec!["半", "完"]);
        assert_eq!(b.outstanding(), 0);
    }

    #[test]
    fn delay_only_for_live_model() {
        let v: Value = serde_json::from_str(&session_update(&SessionConfig::from_whisper(
            "gpt-transcribe",
            Some("zh-TW"),
            None,
        )))
        .unwrap();
        assert!(v["session"]["audio"]["input"]["transcription"]
            .get("delay")
            .is_none());
    }

    #[test]
    fn upsampler_is_3_to_2_and_continuous_across_chunks() {
        let ramp: Vec<f32> = (0..1600).map(|i| i as f32 / 1600.0).collect();
        let mut whole = Upsampler::default();
        let one = whole.process(&ramp);
        let mut split = Upsampler::default();
        let mut two = split.process(&ramp[..801]);
        two.extend(split.process(&ramp[801..]));
        assert!((2399..=2400).contains(&one.len()));
        assert_eq!(one.len(), two.len());
        for (a, b) in one.iter().zip(&two) {
            assert!((a - b).abs() < 1e-5);
        }
        // 線性插值保持單調遞增
        assert!(one.windows(2).all(|w| w[1] >= w[0]));
    }
}
