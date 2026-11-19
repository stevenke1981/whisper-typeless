use std::path::PathBuf;
use tracing::error;

pub(super) fn save_recording_wav(samples: &[f32], sample_rate: u32) -> anyhow::Result<PathBuf> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recordings");
    std::fs::create_dir_all(&base)?;

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = base.join(format!("ptt-{millis}.wav"));
    let mut file = std::fs::File::create(&path)?;

    let data_len = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        file.write_all(&pcm.to_le_bytes())?;
    }

    Ok(path)
}

pub(super) fn inject_text_to_focused_window(text: &str) {
    use enigo::{Enigo, Keyboard, Settings};
    match Enigo::new(&Settings::default()) {
        Ok(mut enigo) => {
            let _ = enigo.text(text);
        }
        Err(e) => {
            error!("enigo 初始化失敗: {e}");
        }
    }
}

pub(super) fn current_time_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    // 轉換為 UTC+8（台灣時間）
    let h_tw = (h + 8) % 24;
    format!("{:02}:{:02}:{:02}", h_tw, m, s)
}
