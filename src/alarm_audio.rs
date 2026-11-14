//! 铃声播放：Windows 上用 winmm 的 MCI 播放 mp3/wav，MCI 打不开时退回 PlaySoundW（只支持 wav）。
//!
//! 注意：MCI 调用（尤其 open / stop / status）会走音频驱动，可能在 UI 线程上阻塞几百毫秒甚至更久，
//! 这会让岛仔整帧停住、动画卡在半路。所以这里所有 MCI 调用都放在一个独立工作线程里，
//! UI 线程只往 channel 投递命令、读一个 Arc<Mutex<..>> 里的状态，永远不碰驱动。

#[cfg(windows)]
pub use imp::Player;

#[cfg(not(windows))]
pub use imp::Player;

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn mciSendStringW(cmd: *const u16, ret: *mut u16, ret_len: u32, hwnd: *mut std::ffi::c_void) -> u32;
        fn PlaySoundW(name: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
    }

    const ALIAS: &str = "dzalarm";
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_LOOP: u32 = 0x0008;
    const SND_FILENAME: u32 = 0x0002_0000;

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn mci(cmd: &str) -> Result<(), u32> {
        let cmd = wide(cmd);
        let code = unsafe { mciSendStringW(cmd.as_ptr(), std::ptr::null_mut(), 0, std::ptr::null_mut()) };
        if code == 0 { Ok(()) } else { Err(code) }
    }

    fn mci_query(cmd: &str) -> String {
        let cmd = wide(cmd);
        let mut buf = [0u16; 64];
        let code = unsafe { mciSendStringW(cmd.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, std::ptr::null_mut()) };
        if code != 0 {
            return String::new();
        }
        let len = buf.iter().position(|v| *v == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len]).trim().to_string()
    }

    fn play_sound(path: &str, looping: bool) -> bool {
        let path = wide(path);
        let flags = SND_ASYNC | SND_FILENAME | SND_NODEFAULT | if looping { SND_LOOP } else { 0 };
        unsafe { PlaySoundW(path.as_ptr(), std::ptr::null_mut(), flags) != 0 }
    }

    fn stop_sound() {
        unsafe {
            PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
        }
    }

    enum Job {
        Play { path: String, volume: u8, looping: bool },
        Stop,
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct Shared {
        /// 应该出声（用户没点停止、也没到静音时刻）。
        wanted: bool,
        /// 这一刻确实在出声。
        playing: bool,
    }

    /// 只保存 channel 与共享状态；所有驱动调用都在工作线程里。
    pub struct Player {
        tx: Option<Sender<Job>>,
        shared: Arc<Mutex<Shared>>,
    }

    impl std::fmt::Debug for Player {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Player").field("shared", &*self.shared.lock().unwrap_or_else(|e| e.into_inner())).finish()
        }
    }

    impl Default for Player {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Player {
        pub fn new() -> Self {
            let shared = Arc::new(Mutex::new(Shared::default()));
            let (tx, rx) = channel::<Job>();
            let state = Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name("daozai-alarm-audio".into())
                .spawn(move || worker(rx, state));
            Self { tx: if spawned.is_ok() { Some(tx) } else { None }, shared }
        }

        /// 是否正在出声（UI 用这个决定要不要显示「响铃中」）。
        pub fn active(&self) -> bool {
            self.shared.lock().map(|state| state.playing).unwrap_or(false)
        }

        /// 当前是否「应该响」（点过停止之后才会变 false）。
        pub fn wanted(&self) -> bool {
            self.shared.lock().map(|state| state.wanted).unwrap_or(false)
        }

        pub fn start(&mut self, path: &str, volume: u8, looping: bool) {
            let path = path.trim().to_string();
            if let Ok(mut state) = self.shared.lock() {
                state.wanted = !path.is_empty();
                state.playing = false;
            }
            if let Some(tx) = &self.tx {
                let _ = tx.send(Job::Play { path, volume: volume.min(100), looping });
            }
        }

        pub fn stop(&mut self) {
            if let Ok(mut state) = self.shared.lock() {
                state.wanted = false;
                state.playing = false;
            }
            if let Some(tx) = &self.tx {
                let _ = tx.send(Job::Stop);
            }
        }

        /// 兼容旧调用点：轮询已经搬到工作线程，这里什么都不做。
        pub fn tick(&mut self, _now: f64) {}
    }

    fn open_and_play(path: &str, volume: u8) -> bool {
        let _ = mci(&format!("close {ALIAS}"));
        let quoted = path.replace('"', "");
        let opened = mci(&format!("open \"{quoted}\" type mpegvideo alias {ALIAS}")).is_ok()
            || mci(&format!("open \"{quoted}\" type waveaudio alias {ALIAS}")).is_ok();
        if !opened {
            return false;
        }
        let _ = mci(&format!("setaudio {ALIAS} volume to {}", (volume.min(100) as u32) * 10));
        mci(&format!("play {ALIAS}")).is_ok()
    }

    fn worker(rx: Receiver<Job>, shared: Arc<Mutex<Shared>>) {
        let mut looping = false;
        let mut playing = false;
        let mut via_sound = false;
        let mut path = String::new();
        let mut volume = 80u8;
        loop {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(Job::Play { path: p, volume: v, looping: l }) => {
                    path = p;
                    volume = v;
                    looping = l;
                    via_sound = false;
                    playing = false;
                    if !path.is_empty() {
                        playing = open_and_play(&path, volume);
                        if !playing && path.to_ascii_lowercase().ends_with(".wav") && play_sound(&path, looping) {
                            via_sound = true;
                            playing = true;
                        }
                    }
                    if let Ok(mut state) = shared.lock() {
                        state.playing = playing;
                    }
                }
                Ok(Job::Stop) => {
                    let _ = mci(&format!("stop {ALIAS}"));
                    let _ = mci(&format!("close {ALIAS}"));
                    if via_sound {
                        stop_sound();
                    }
                    via_sound = false;
                    playing = false;
                    if let Ok(mut state) = shared.lock() {
                        state.playing = false;
                        state.wanted = false;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if !playing {
                        continue;
                    }
                    if via_sound {
                        // PlaySound 自己会循环，没法查询状态，只能等 Stop。
                        continue;
                    }
                    let mode = mci_query(&format!("status {ALIAS} mode"));
                    // 播完了（mode 变成 stopped）：循环就从头上再放一遍，否则结束。
                    if mode != "playing" {
                        if looping {
                            let _ = mci(&format!("seek {ALIAS} to start"));
                            let _ = open_and_play_seek(&path, volume);
                            if let Ok(mut state) = shared.lock() {
                                state.playing = playing;
                            }
                        } else {
                            let _ = mci(&format!("close {ALIAS}"));
                            playing = false;
                            if let Ok(mut state) = shared.lock() {
                                state.playing = false;
                            }
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = mci(&format!("stop {ALIAS}"));
        let _ = mci(&format!("close {ALIAS}"));
        if via_sound {
            stop_sound();
        }
    }

    /// 循环重放：MCI 播完时位置停在末尾，直接 play 不会出声，必须 seek 回开头。
    fn open_and_play_seek(path: &str, volume: u8) -> bool {
        let _ = mci(&format!("seek {ALIAS} to start"));
        let _ = mci(&format!("setaudio {ALIAS} volume to {}", (volume.min(100) as u32) * 10));
        if mci(&format!("play {ALIAS}")).is_ok() {
            true
        } else {
            // 设备被回收了，重新打开再放。
            open_and_play(path, volume)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    #[derive(Clone, Copy, Debug, Default)]
    struct Shared;

    #[derive(Debug, Default)]
    pub struct Player;

    impl Player {
        pub fn new() -> Self {
            Self
        }
        pub fn active(&self) -> bool {
            false
        }
        pub fn wanted(&self) -> bool {
            false
        }
        pub fn start(&mut self, _path: &str, _volume: u8, _looping: bool) {}
        pub fn stop(&mut self) {}
        pub fn tick(&mut self, _now: f64) {}
    }
}
