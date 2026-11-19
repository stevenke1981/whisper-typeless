//! 雲端轉錄。只有使用者在設定中選擇 OpenAI 引擎並明確同意（`openai_consent`）、
//! 且未開啟隱私模式時，[`Controller`](crate::core::Controller) 才會選用；預設一律本機轉錄。

pub mod openai;
