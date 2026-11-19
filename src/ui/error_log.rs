//! WARN / ERROR 事件 → 有界環形緩衝 → UI 錯誤紀錄浮層。
//!
//! [`ErrorLogLayer`] 是一個 tracing Layer，於 `main.rs` 安裝；
//! UI 端透過 [`ErrorLog::set_listener`] 取得變更通知（見 `bridge/window.rs`）。

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

/// 預設保留的錯誤筆數
pub const DEFAULT_CAPACITY: usize = 200;

type Listener = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    capacity: usize,
    entries: Mutex<VecDeque<String>>,
    listener: Mutex<Option<Listener>>,
}

/// 執行緒安全、有界（超過容量時丟棄最舊）的錯誤紀錄。可廉價 clone（共享同一份）。
#[derive(Clone)]
pub struct ErrorLog {
    inner: Arc<Inner>,
}

impl ErrorLog {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Arc::new(Inner {
                capacity,
                entries: Mutex::new(VecDeque::with_capacity(capacity)),
                listener: Mutex::new(None),
            }),
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    pub fn push(&self, entry: impl Into<String>) {
        {
            let mut entries = lock(&self.inner.entries);
            while entries.len() >= self.inner.capacity {
                entries.pop_front();
            }
            entries.push_back(entry.into());
        }
        self.notify();
    }

    /// 由舊到新
    pub fn snapshot(&self) -> Vec<String> {
        lock(&self.inner.entries).iter().cloned().collect()
    }

    /// 由新到舊（UI 顯示用：最新的錯誤在最上方，不會被浮層高度截掉）
    pub fn snapshot_newest_first(&self) -> Vec<String> {
        lock(&self.inner.entries).iter().rev().cloned().collect()
    }

    pub fn len(&self) -> usize {
        lock(&self.inner.entries).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        lock(&self.inner.entries).clear();
        self.notify();
    }

    /// 設定變更通知（push / clear 後呼叫，呼叫時不持有內部鎖）。
    /// 注意：listener 內不可再發出 WARN/ERROR 事件，以免遞迴。
    pub fn set_listener(&self, listener: impl Fn() + Send + Sync + 'static) {
        *lock(&self.inner.listener) = Some(Arc::new(listener));
    }

    pub fn clear_listener(&self) {
        *lock(&self.inner.listener) = None;
    }

    fn notify(&self) {
        let listener = lock(&self.inner.listener).clone();
        if let Some(listener) = listener {
            listener();
        }
    }
}

/// tracing 回呼中不可 panic：鎖中毒時直接取回內容。
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 全域錯誤紀錄（容量 [`DEFAULT_CAPACITY`]）
pub fn global() -> &'static ErrorLog {
    static GLOBAL: OnceLock<ErrorLog> = OnceLock::new();
    GLOBAL.get_or_init(|| ErrorLog::new(DEFAULT_CAPACITY))
}

/// 寫入全域錯誤紀錄的 Layer
pub fn layer() -> ErrorLogLayer {
    ErrorLogLayer::new(global().clone())
}

/// 擷取 WARN / ERROR 事件的 tracing Layer
pub struct ErrorLogLayer {
    log: ErrorLog,
}

impl ErrorLogLayer {
    pub fn new(log: ErrorLog) -> Self {
        Self { log }
    }
}

impl<S: Subscriber> Layer<S> for ErrorLogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        // tracing 的 Level 依詳細程度排序：ERROR < WARN < INFO
        if level > Level::WARN {
            return;
        }

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let tag = if level == Level::ERROR {
            "錯誤"
        } else {
            "警告"
        };
        self.log
            .push(format!("{} [{tag}] {}", time_str(), visitor.finish()));
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl MessageVisitor {
    fn finish(self) -> String {
        match (self.message.is_empty(), self.fields.is_empty()) {
            (_, true) => self.message,
            (true, false) => self.fields,
            (false, false) => format!("{} ({})", self.message, self.fields),
        }
    }

    fn push_field(&mut self, field: &Field, value: std::fmt::Arguments<'_>) {
        if !self.fields.is_empty() {
            self.fields.push_str(", ");
        }
        let _ = write!(self.fields, "{}={}", field.name(), value);
    }
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            self.push_field(field, format_args!("{value}"));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            self.push_field(field, format_args!("{value:?}"));
        }
    }
}

/// HH:MM:SS（UTC+8，與轉錄紀錄時間戳一致）
fn time_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 8 * 3600;
    format!(
        "{:02}:{:02}:{:02}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn buffer_is_bounded_and_drops_oldest() {
        let log = ErrorLog::new(3);
        for i in 0..5 {
            log.push(format!("e{i}"));
        }
        assert_eq!(log.len(), 3);
        assert_eq!(log.snapshot(), vec!["e2", "e3", "e4"]);
        assert_eq!(log.snapshot_newest_first(), vec!["e4", "e3", "e2"]);
    }

    #[test]
    fn default_capacity_is_200() {
        let log = ErrorLog::new(DEFAULT_CAPACITY);
        for i in 0..250 {
            log.push(format!("e{i}"));
        }
        assert_eq!(log.len(), 200);
        assert_eq!(log.snapshot().first().map(String::as_str), Some("e50"));
        assert_eq!(log.snapshot().last().map(String::as_str), Some("e249"));
    }

    #[test]
    fn clear_and_listener_notified() {
        let log = ErrorLog::new(10);
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        log.set_listener(move || {
            h.fetch_add(1, Ordering::SeqCst);
        });
        log.push("a");
        log.push("b");
        log.clear();
        assert!(log.is_empty());
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        log.clear_listener();
        log.push("c");
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn listener_can_read_log_without_deadlock() {
        let log = ErrorLog::new(10);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (l, s) = (log.clone(), seen.clone());
        log.set_listener(move || {
            *s.lock().unwrap() = l.snapshot();
        });
        log.push("x");
        assert_eq!(*seen.lock().unwrap(), vec!["x"]);
    }

    #[test]
    fn layer_captures_only_warn_and_error_in_order() {
        let log = ErrorLog::new(10);
        let subscriber = tracing_subscriber::registry().with(ErrorLogLayer::new(log.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::trace!("t");
            tracing::debug!("d");
            tracing::info!("i");
            tracing::warn!("第一個警告");
            tracing::error!(code = 7, "模型載入失敗: {}", "boom");
            tracing::warn!(path = "a.wav");
        });
        let entries = log.snapshot();
        assert_eq!(entries.len(), 3, "{entries:?}");
        assert!(entries[0].ends_with("[警告] 第一個警告"), "{}", entries[0]);
        assert!(
            entries[1].ends_with("[錯誤] 模型載入失敗: boom (code=7)"),
            "{}",
            entries[1]
        );
        assert!(entries[2].ends_with("[警告] path=a.wav"), "{}", entries[2]);
    }
}
