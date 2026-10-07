//! Delivering the English text into the target app.
//!
//! All clipboard and synthetic-input work happens on one dedicated thread:
//! - on Linux the clipboard owner has to stay alive for the content to remain
//!   available, so the `arboard::Clipboard` lives as long as the app;
//! - commits are serialized, so two quick Enters can't interleave their
//!   save / paste / restore steps.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use zh2en_core::config::CommitMethod;

use crate::focus::{self, PrevWindow};

pub struct CommitJob {
    pub text: String,
    /// false: only copy to the clipboard.
    pub deliver: bool,
    pub method: CommitMethod,
    pub prev: Option<PrevWindow>,
    pub paste_delay_ms: u64,
    pub restore_clipboard: bool,
    pub reply: mpsc::Sender<Result<(), String>>,
}

#[derive(Clone)]
pub struct Committer {
    tx: mpsc::Sender<CommitJob>,
}

impl Committer {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<CommitJob>();
        thread::Builder::new()
            .name("zh2en-commit".into())
            .spawn(move || {
                let mut clipboard = arboard::Clipboard::new().ok();
                for job in rx {
                    let res = run(&mut clipboard, &job);
                    if let Err(e) = &res {
                        tracing::error!("commit failed: {e}");
                    }
                    let _ = job.reply.send(res);
                }
            })
            .expect("spawn commit thread");
        Self { tx }
    }

    /// Queue a job and wait for it on a blocking-friendly thread.
    pub async fn commit(&self, job_fn: impl FnOnce(mpsc::Sender<Result<(), String>>) -> CommitJob) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx.send(job_fn(reply_tx)).map_err(|_| "commit thread stopped".to_string())?;
        tauri::async_runtime::spawn_blocking(move || reply_rx.recv().unwrap_or_else(|_| Err("commit thread stopped".into())))
            .await
            .map_err(|e| e.to_string())?
    }
}

fn run(clipboard: &mut Option<arboard::Clipboard>, job: &CommitJob) -> Result<(), String> {
    if clipboard.is_none() {
        *clipboard = Some(arboard::Clipboard::new().map_err(|e| format!("无法访问剪贴板：{e}"))?);
    }
    let cb = clipboard.as_mut().unwrap();

    if !job.deliver {
        return cb.set_text(job.text.clone()).map_err(|e| format!("写入剪贴板失败：{e}"));
    }

    focus::restore(job.prev);
    // Give the OS time to finish switching focus to the target window.
    thread::sleep(Duration::from_millis(job.paste_delay_ms));

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("无法模拟键盘输入：{e}（macOS 需在“系统设置 › 隐私与安全性 › 辅助功能”中授权）"))?;

    match job.method {
        CommitMethod::Type => enigo.text(&job.text).map_err(|e| format!("模拟输入失败：{e}")),
        CommitMethod::Paste => {
            let saved = if job.restore_clipboard { cb.get_text().ok() } else { None };
            cb.set_text(job.text.clone()).map_err(|e| format!("写入剪贴板失败：{e}"))?;
            thread::sleep(Duration::from_millis(30));
            send_paste_chord(&mut enigo).map_err(|e| format!("发送粘贴快捷键失败：{e}"))?;
            if let Some(old) = saved {
                // The target app reads the clipboard asynchronously after the
                // keystroke; restoring too early would paste the old text.
                thread::sleep(Duration::from_millis(400));
                let _ = cb.set_text(old);
            }
            Ok(())
        }
    }
}

fn send_paste_chord(enigo: &mut Enigo) -> enigo::InputResult<()> {
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press)?;
    let res = enigo.key(Key::Unicode('v'), Direction::Click);
    // Always release the modifier, even if the V press failed.
    enigo.key(modifier, Direction::Release)?;
    res
}
