//! 轉錄工作階段：送音訊、依本機 VAD commit、收結果，停止時有限等待尚未完成的語句。
//!
//! 只依賴 [`Transport`]，測試以通道模擬伺服器，不需網路。

use std::time::{Duration, Instant};

use super::protocol::{
    append_event, commit_event, parse_server_event, session_update, ReorderBuffer, ServerEvent,
    SessionConfig, FRAME_SAMPLES, MIN_COMMIT_SAMPLES,
};
use super::transport::Transport;

/// 工作階段交給錄音管線的結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionOutput {
    /// 暫時文字（尚未完成的語句）。
    Partial(String),
    /// 一段完成的語句（依 commit 順序）。
    Final(String),
    /// 錯誤（zh-TW 訊息，不含金鑰或轉錄內容）。
    Failed(String),
}

/// 停止時每次等待伺服器事件的間隔。
const FINISH_POLL: Duration = Duration::from_millis(50);

pub struct CloudSession {
    transport: Box<dyn Transport>,
    /// 尚未送出的樣本（不足一個封包）。
    pending: Vec<f32>,
    /// 上次 commit 之後已送出的樣本數。
    uncommitted: usize,
    commits_sent: usize,
    commits_acked: usize,
    reorder: ReorderBuffer,
    last_partial: String,
    /// 傳輸層失敗訊息；設定後不再送出任何東西。
    dead: Option<String>,
    /// 收到過伺服器的錯誤 / 語句失敗。
    had_error: bool,
    /// 已交出至少一段非空白的完成語句。
    delivered: bool,
}

impl CloudSession {
    /// 送出 `session.update` 開始工作階段（不等待回應）。
    pub fn start(mut transport: Box<dyn Transport>, cfg: &SessionConfig) -> Self {
        let dead = transport.send(session_update(cfg)).err();
        Self {
            transport,
            pending: Vec::new(),
            uncommitted: 0,
            commits_sent: 0,
            commits_acked: 0,
            reorder: ReorderBuffer::default(),
            last_partial: String::new(),
            dead,
            had_error: false,
            delivered: false,
        }
    }

    /// 連線已失敗。
    pub fn is_dead(&self) -> bool {
        self.dead.is_some()
    }

    /// 發生過任何錯誤（連線、伺服器錯誤或語句失敗）。
    pub fn had_error(&self) -> bool {
        self.dead.is_some() || self.had_error
    }

    /// 已輸出過至少一段文字。
    pub fn delivered_any(&self) -> bool {
        self.delivered
    }

    fn send(&mut self, text: String) {
        if self.dead.is_some() {
            return;
        }
        if let Err(e) = self.transport.send(text) {
            self.dead = Some(e);
        }
    }

    fn send_frame(&mut self, samples: &[f32]) {
        self.uncommitted += samples.len();
        self.send(append_event(samples));
    }

    /// 加入 16 kHz 音訊；每滿 100 ms 送出一個封包。
    pub fn push_audio(&mut self, samples: &[f32]) {
        if self.dead.is_some() {
            return;
        }
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.pending.drain(..FRAME_SAMPLES).collect();
            self.send_frame(&frame);
        }
    }

    /// 把目前的音訊結成一段語句（本機 VAD 偵測到段落結束 / 停止錄音時）。
    /// 沒有新音訊時不送；不足 100 ms 時補靜音。回傳是否送出 commit。
    pub fn commit(&mut self) -> bool {
        if self.dead.is_some() {
            return false;
        }
        if !self.pending.is_empty() {
            let frame = std::mem::take(&mut self.pending);
            self.send_frame(&frame);
        }
        if self.uncommitted == 0 {
            return false;
        }
        if self.uncommitted < MIN_COMMIT_SAMPLES {
            let pad = vec![0.0; MIN_COMMIT_SAMPLES - self.uncommitted];
            self.send_frame(&pad);
        }
        self.send(commit_event());
        self.uncommitted = 0;
        if self.dead.is_some() {
            return false;
        }
        self.commits_sent += 1;
        true
    }

    /// 收取伺服器事件：先最多等 `timeout`，之後把已到達的全部取完。
    pub fn poll(&mut self, timeout: Duration) -> Vec<SessionOutput> {
        let mut out = Vec::new();
        let mut wait = timeout;
        while self.dead.is_none() {
            match self.transport.recv(wait) {
                Ok(Some(text)) => {
                    if let Some(event) = parse_server_event(&text) {
                        self.handle(event, &mut out);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    out.push(SessionOutput::Failed(format!("雲端轉錄連線失敗：{e}")));
                    self.dead = Some(e);
                }
            }
            wait = Duration::ZERO;
        }
        out
    }

    fn handle(&mut self, event: ServerEvent, out: &mut Vec<SessionOutput>) {
        match event {
            ServerEvent::SessionUpdated | ServerEvent::Other => {}
            ServerEvent::Committed { item_id } => {
                self.commits_acked += 1;
                self.reorder.committed(&item_id);
            }
            ServerEvent::Delta { item_id, delta } => {
                self.reorder.delta(&item_id, &delta);
                self.emit_partial(out);
            }
            ServerEvent::Completed {
                item_id,
                transcript,
            } => {
                let released = self.reorder.completed(&item_id, &transcript);
                self.emit_finals(released, out);
            }
            ServerEvent::Failed { item_id, message } => {
                self.had_error = true;
                out.push(SessionOutput::Failed(format!("雲端轉錄失敗：{message}")));
                let released = self.reorder.failed(&item_id);
                self.emit_finals(released, out);
            }
            ServerEvent::Error { message } => {
                self.had_error = true;
                // 錯誤可能代表某次 commit 不會被確認：不再等待尚未確認的 commit。
                self.commits_acked = self.commits_sent;
                out.push(SessionOutput::Failed(format!("OpenAI 錯誤：{message}")));
            }
        }
    }

    fn emit_finals(&mut self, released: Vec<String>, out: &mut Vec<SessionOutput>) {
        for text in released {
            if !text.trim().is_empty() {
                self.delivered = true;
                out.push(SessionOutput::Final(text));
            }
        }
        self.emit_partial(out);
    }

    fn emit_partial(&mut self, out: &mut Vec<SessionOutput>) {
        let partial = self.reorder.partial_text();
        if partial != self.last_partial {
            self.last_partial = partial.clone();
            out.push(SessionOutput::Partial(partial));
        }
    }

    /// 尚在等待伺服器的 commit 或語句。
    fn waiting(&self) -> bool {
        self.commits_acked < self.commits_sent || self.reorder.outstanding() > 0
    }

    /// 停止錄音：送出最後的 commit，最多等 `max_wait` 讓尚未完成的語句回來，然後關閉連線。
    /// 無論伺服器是否回應，一定會在期限內返回。
    pub fn finish(&mut self, max_wait: Duration) -> Vec<SessionOutput> {
        self.commit();
        let deadline = Instant::now() + max_wait;
        let mut out = self.poll(Duration::ZERO);
        while self.dead.is_none() && self.waiting() {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            out.extend(self.poll(FINISH_POLL.min(deadline - now)));
        }
        if self.reorder.outstanding() > 0 {
            if self.dead.is_none() {
                tracing::warn!("OpenAI 轉錄逾時，部分語句以暫時文字輸出");
            }
            let rest = self.reorder.drain_all();
            self.emit_finals(rest, &mut out);
        }
        self.transport.close();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    /// 通道模擬的傳輸層：記錄送出的訊息；測試端用 `server` 推送伺服器事件。
    struct MockTransport {
        sent: Arc<Mutex<Vec<String>>>,
        rx: mpsc::Receiver<Result<String, String>>,
        fail_send: bool,
        closed: Arc<Mutex<bool>>,
    }

    impl Transport for MockTransport {
        fn send(&mut self, text: String) -> Result<(), String> {
            if self.fail_send {
                return Err("斷線".into());
            }
            self.sent.lock().unwrap().push(text);
            Ok(())
        }
        fn recv(&mut self, timeout: Duration) -> Result<Option<String>, String> {
            match self.rx.recv_timeout(timeout.max(Duration::from_millis(1))) {
                Ok(r) => r.map(Some),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
                Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            }
        }
        fn close(&mut self) {
            *self.closed.lock().unwrap() = true;
        }
    }

    struct Harness {
        sent: Arc<Mutex<Vec<String>>>,
        server: mpsc::Sender<Result<String, String>>,
        closed: Arc<Mutex<bool>>,
    }

    impl Harness {
        fn types(&self) -> Vec<String> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .map(|t| {
                    let v: Value = serde_json::from_str(t).unwrap();
                    v["type"].as_str().unwrap().to_string()
                })
                .collect()
        }
        fn push(&self, json: &str) {
            self.server.send(Ok(json.to_string())).unwrap();
        }
        fn committed(&self, id: &str) {
            self.push(&format!(
                r#"{{"type":"input_audio_buffer.committed","item_id":"{id}"}}"#
            ));
        }
        fn delta(&self, id: &str, d: &str) {
            self.push(&format!(
                r#"{{"type":"conversation.item.input_audio_transcription.delta","item_id":"{id}","delta":"{d}"}}"#
            ));
        }
        fn completed(&self, id: &str, t: &str) {
            self.push(&format!(
                r#"{{"type":"conversation.item.input_audio_transcription.completed","item_id":"{id}","transcript":"{t}"}}"#
            ));
        }
    }

    fn session_with(fail_send: bool) -> (CloudSession, Harness) {
        let (tx, rx) = mpsc::channel();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let closed = Arc::new(Mutex::new(false));
        let t = MockTransport {
            sent: Arc::clone(&sent),
            rx,
            fail_send,
            closed: Arc::clone(&closed),
        };
        let cfg = SessionConfig::from_whisper("gpt-live-transcribe", Some("zh"), None);
        (
            CloudSession::start(Box::new(t), &cfg),
            Harness {
                sent,
                server: tx,
                closed,
            },
        )
    }

    fn session() -> (CloudSession, Harness) {
        session_with(false)
    }

    fn finals(out: &[SessionOutput]) -> Vec<String> {
        out.iter()
            .filter_map(|o| match o {
                SessionOutput::Final(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn sends_session_update_then_100ms_frames() {
        let (mut s, h) = session();
        s.push_audio(&vec![0.1; 1000]);
        assert_eq!(h.types(), vec!["session.update"]);
        s.push_audio(&vec![0.1; 2500]);
        // 3500 樣本 → 兩個 1600 封包，剩 300 等待
        assert_eq!(
            h.types(),
            vec![
                "session.update",
                "input_audio_buffer.append",
                "input_audio_buffer.append"
            ]
        );
        // commit 先送出剩下的 300 樣本，總長已超過 100 ms，不需補靜音
        assert!(s.commit());
        assert_eq!(
            h.types()[3..],
            ["input_audio_buffer.append", "input_audio_buffer.commit"]
        );
        // 沒有新音訊時不重複 commit
        assert!(!s.commit());
    }

    #[test]
    fn short_commit_is_padded_to_minimum() {
        let (mut s, h) = session();
        s.push_audio(&[0.2; 10]);
        assert!(s.commit());
        let sent = h.sent.lock().unwrap().clone();
        use base64::Engine as _;
        let total: usize = sent
            .iter()
            .filter_map(|t| {
                let v: Value = serde_json::from_str(t).unwrap();
                v["audio"].as_str().map(|a| {
                    base64::engine::general_purpose::STANDARD
                        .decode(a)
                        .unwrap()
                        .len()
                        / 2
                })
            })
            .sum();
        assert_eq!(total, MIN_COMMIT_SAMPLES);
    }

    #[test]
    fn deltas_become_partials_and_finals_follow_commit_order() {
        let (mut s, h) = session();
        h.committed("a");
        h.committed("b");
        h.delta("a", "你");
        h.delta("b", "世");
        h.completed("b", "世界");
        h.delta("a", "好");
        let out = s.poll(Duration::from_millis(10));
        assert!(finals(&out).is_empty());
        assert_eq!(out.last(), Some(&SessionOutput::Partial("你好世界".into())));
        h.completed("a", "你好");
        let out = s.poll(Duration::from_millis(10));
        assert_eq!(finals(&out), vec!["你好", "世界"]);
        assert_eq!(out.last(), Some(&SessionOutput::Partial(String::new())));
        assert!(s.delivered_any());
        assert!(!s.had_error());
    }

    #[test]
    fn finish_commits_and_waits_for_outstanding() {
        let (mut s, h) = session();
        s.push_audio(&vec![0.1; 3200]);
        let server = h.server.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            for msg in [
                r#"{"type":"input_audio_buffer.committed","item_id":"z"}"#,
                r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"z","transcript":"結束"}"#,
            ] {
                server.send(Ok(msg.to_string())).unwrap();
            }
        });
        let out = s.finish(Duration::from_secs(5));
        t.join().unwrap();
        assert_eq!(finals(&out), vec!["結束"]);
        assert_eq!(h.types().last().unwrap(), "input_audio_buffer.commit");
        assert!(*h.closed.lock().unwrap());
    }

    #[test]
    fn finish_is_bounded_when_server_is_silent() {
        let (mut s, h) = session();
        s.push_audio(&vec![0.1; 1600]);
        h.committed("q");
        h.delta("q", "半句");
        let started = Instant::now();
        let out = s.finish(Duration::from_millis(150));
        assert!(started.elapsed() < Duration::from_secs(2));
        // 逾時後以暫時文字輸出，不丟內容
        assert_eq!(finals(&out), vec!["半句"]);
        assert!(*h.closed.lock().unwrap());
    }

    #[test]
    fn server_error_stops_waiting_for_commit() {
        let (mut s, h) = session();
        s.push_audio(&vec![0.1; 1600]);
        h.push(r#"{"type":"error","error":{"message":"invalid model"}}"#);
        let started = Instant::now();
        let out = s.finish(Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(out.contains(&SessionOutput::Failed("OpenAI 錯誤：invalid model".into())));
        assert!(s.had_error());
        assert!(!s.delivered_any());
    }

    #[test]
    fn failed_item_reports_and_unblocks() {
        let (mut s, h) = session();
        h.committed("1");
        h.committed("2");
        h.completed("2", "第二");
        h.push(r#"{"type":"conversation.item.input_audio_transcription.failed","item_id":"1","error":{"message":"x"}}"#);
        let out = s.poll(Duration::from_millis(10));
        assert!(out.contains(&SessionOutput::Failed("雲端轉錄失敗：x".into())));
        assert_eq!(finals(&out), vec!["第二"]);
    }

    #[test]
    fn transport_failure_marks_dead() {
        let (mut s, _h) = session_with(true);
        assert!(s.is_dead());
        s.push_audio(&vec![0.1; 5000]);
        assert!(!s.commit());
        let started = Instant::now();
        let out = s.finish(Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(finals(&out).is_empty());
        assert!(s.had_error());
    }

    #[test]
    fn recv_error_reports_failure_once() {
        let (mut s, h) = session();
        h.server
            .send(Err("OpenAI API 金鑰無效或已失效（401）".into()))
            .unwrap();
        let out = s.poll(Duration::from_millis(10));
        assert_eq!(
            out,
            vec![SessionOutput::Failed(
                "雲端轉錄連線失敗：OpenAI API 金鑰無效或已失效（401）".into()
            )]
        );
        assert!(s.is_dead());
        assert!(s.poll(Duration::from_millis(1)).is_empty());
    }

    #[test]
    fn unknown_events_are_ignored() {
        let (mut s, h) = session();
        h.push(r#"{"type":"session.created"}"#);
        h.push("garbage");
        h.push(r#"{"type":"session.updated"}"#);
        assert!(s.poll(Duration::from_millis(10)).is_empty());
        assert!(!s.had_error());
    }
}
