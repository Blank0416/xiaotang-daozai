//! 铃声播放：Windows 上用 winmm 的 MCI 播放 mp3/wav，MCI 打不开时退回 PlaySoundW（只支持 wav）。

#[cfg(windows)]
pub use imp::Player;

#[cfg(not(windows))]
pub use stub::Player;

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;

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
        if code == 0 {
            Ok(())
        } else {
            Err(code)
        }
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

    #[derive(Clone, Debug)]
    pub struct Player {
        path: String,
        volume: u8,
        looping: bool,
        active: bool,
        via_sound: bool,
        last_check: f64,
    }

    impl Default for Player {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Player {
        pub fn new() -> Self {
            Self { path: String::new(), volume: 80, looping: true, active: false, via_sound: false, last_check: 0.0 }
        }

        pub fn active(&self) -> bool {
            self.active
        }

        /// 开始播放；path 为空时什么都不做。
        pub fn start(&mut self, path: &str, volume: u8, looping: bool) {
            self.stop();
            self.path = path.trim().to_string();
            self.volume = volume.min(100);
            self.looping = looping;
            if self.path.is_empty() {
                return;
            }
            if self.start_mci() {
                self.active = true;
                return;
            }
            // MCI 打不开（少见编码 / 文件被占用）：wav 还能用 PlaySound 循环播放。
            if self.path.to_ascii_lowercase().ends_with(".wav") && play_sound(&self.path, looping) {
                self.via_sound = true;
                self.active = true;
            }
        }

        fn start_mci(&mut self) -> bool {
            let _ = mci(&format!("close {ALIAS}"));
            let quoted = self.path.replace('"', "");
            let opened = mci(&format!("open \"{quoted}\" type mpegvideo alias {ALIAS}")).is_ok()
                || mci(&format!("open \"{quoted}\" type waveaudio alias {ALIAS}")).is_ok();
            if !opened {
                return false;
            }
            let _ = mci(&format!("setaudio {ALIAS} volume to {}", self.volume as u32 * 10));
            mci(&format!("play {ALIAS}")).is_ok()
        }

        /// 每帧调用；铃声播完时按需要续播。
        pub fn tick(&mut self, now: f64) {
            if !self.active || self.via_sound || now - self.last_check < 0.5 {
                return;
            }
            self.last_check = now;
            if mci_query(&format!("status {ALIAS} mode")) == "playing" {
                return;
            }
            if self.looping {
                let _ = mci(&format!("play {ALIAS}"));
            } else {
                self.stop();
            }
        }

        pub fn set_volume(&mut self, volume: u8) {
            self.volume = volume.min(100);
            if self.active && !self.via_sound {
                let _ = mci(&format!("setaudio {ALIAS} volume to {}", self.volume as u32 * 10));
            }
        }

        pub fn stop(&mut self) {
            if self.via_sound {
                stop_sound();
                self.via_sound = false;
            }
            if self.active {
                let _ = mci(&format!("stop {ALIAS}"));
                let _ = mci(&format!("close {ALIAS}"));
            }
            self.active = false;
        }
    }
}

#[cfg(not(windows))]
mod stub {
    #[derive(Clone, Debug, Default)]
    pub struct Player;

    impl Player {
        pub fn new() -> Self {
            Self
        }
        pub fn active(&self) -> bool {
            false
        }
        pub fn start(&mut self, _path: &str, _volume: u8, _looping: bool) {}
        pub fn tick(&mut self, _now: f64) {}
        pub fn set_volume(&mut self, _volume: u8) {}
        pub fn stop(&mut self) {}
    }
}
