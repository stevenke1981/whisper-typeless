//! 傳輸層：[`Transport`] trait（工作階段邏輯只依賴它，測試用通道模擬）與真正的
//! WebSocket 實作 [`WsTransport`]（tokio-tungstenite + rustls，薄轉接層）。

use std::sync::mpsc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::runtime::Handle as TokioHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use super::protocol::REALTIME_URL;
use crate::settings::Secret;

/// 以文字訊息往返的雙向通道。
pub trait Transport: Send {
    /// 送出一則文字訊息（不阻塞；連線建立前會先排隊）。
    fn send(&mut self, text: String) -> Result<(), String>;
    /// 最多等 `timeout` 收一則文字訊息；逾時回 `Ok(None)`；連線失敗或已關閉回 `Err`。
    fn recv(&mut self, timeout: Duration) -> Result<Option<String>, String>;
    /// 關閉連線。
    fn close(&mut self);
}

/// 真正的 OpenAI Realtime WebSocket 連線。連線在 tokio 背景工作中建立與收送，
/// 因此 [`WsTransport::connect`] 立即返回、不會卡住畫面執行緒；失敗會由 [`Transport::recv`] 回報。
pub struct WsTransport {
    out_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    in_rx: mpsc::Receiver<Result<String, String>>,
    closed: Option<String>,
}

fn describe_ws_error(e: &WsError) -> String {
    match e {
        WsError::Http(resp) => match resp.status().as_u16() {
            401 => "OpenAI API 金鑰無效或已失效（401）".into(),
            403 => "OpenAI 拒絕存取（403），請確認帳號權限".into(),
            429 => "OpenAI 請求過多或額度不足（429）".into(),
            code => format!("OpenAI 連線失敗（HTTP {code}）"),
        },
        other => format!("無法連線到 OpenAI：{other}"),
    }
}

impl WsTransport {
    /// 在 `tokio` 上開始連線。金鑰只放進 `Authorization` 標頭（標記為敏感），不寫入日誌。
    pub fn connect(tokio: &TokioHandle, api_key: &Secret) -> Self {
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (in_tx, in_rx) = mpsc::channel::<Result<String, String>>();

        let request = (|| {
            let mut req = REALTIME_URL.into_client_request().ok()?;
            let mut value = HeaderValue::from_str(&format!("Bearer {}", api_key.expose())).ok()?;
            value.set_sensitive(true);
            req.headers_mut().insert("Authorization", value);
            Some(req)
        })();

        tokio.spawn(async move {
            let request = match request {
                Some(r) => r,
                None => {
                    let _ = in_tx.send(Err("OpenAI API 金鑰格式不正確".into()));
                    return;
                }
            };
            let ws = match tokio_tungstenite::connect_async(request).await {
                Ok((ws, _resp)) => ws,
                Err(e) => {
                    let msg = describe_ws_error(&e);
                    tracing::warn!("{msg}");
                    let _ = in_tx.send(Err(msg));
                    return;
                }
            };
            let (mut sink, mut stream) = ws.split();
            loop {
                tokio::select! {
                    out = out_rx.recv() => match out {
                        Some(text) => {
                            if let Err(e) = sink.send(Message::text(text)).await {
                                let _ = in_tx.send(Err(describe_ws_error(&e)));
                                break;
                            }
                        }
                        None => {
                            let _ = sink.send(Message::Close(None)).await;
                            break;
                        }
                    },
                    incoming = stream.next() => match incoming {
                        Some(Ok(Message::Text(text))) => {
                            if in_tx.send(Ok(text.to_string())).is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            let _ = in_tx.send(Err("OpenAI 連線已關閉".into()));
                            break;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            let _ = in_tx.send(Err(describe_ws_error(&e)));
                            break;
                        }
                    },
                }
            }
        });

        Self {
            out_tx: Some(out_tx),
            in_rx,
            closed: None,
        }
    }
}

impl Transport for WsTransport {
    fn send(&mut self, text: String) -> Result<(), String> {
        if let Some(msg) = &self.closed {
            return Err(msg.clone());
        }
        match &self.out_tx {
            Some(tx) => tx.send(text).map_err(|_| "OpenAI 連線已關閉".to_string()),
            None => Err("OpenAI 連線已關閉".into()),
        }
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        if let Some(msg) = &self.closed {
            return Err(msg.clone());
        }
        let result = if timeout.is_zero() {
            self.in_rx.try_recv().map_err(|e| match e {
                mpsc::TryRecvError::Empty => None,
                mpsc::TryRecvError::Disconnected => Some(()),
            })
        } else {
            self.in_rx.recv_timeout(timeout).map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => None,
                mpsc::RecvTimeoutError::Disconnected => Some(()),
            })
        };
        match result {
            Ok(Ok(text)) => Ok(Some(text)),
            Ok(Err(msg)) => {
                self.closed = Some(msg.clone());
                Err(msg)
            }
            Err(None) => Ok(None),
            Err(Some(())) => {
                let msg = "OpenAI 連線已關閉".to_string();
                self.closed = Some(msg.clone());
                Err(msg)
            }
        }
    }

    fn close(&mut self) {
        // 丟掉送出端：背景工作送出 Close 後結束。
        self.out_tx = None;
    }
}
