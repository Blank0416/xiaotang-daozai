//! 闹钟：每天 / 工作日 / 周末 / 指定星期 / 一次性 的定时响铃，含贪睡与倒计时。
//!
//! 数据存在 %APPDATA%\Daozai\alarms.txt，一行一个闹钟：
//! a|<开关>|<HH:MM>|<重复位>|<日期>|<贪睡分钟>|<上次响铃>|<贪睡到>|<铃声>|<标题>
//! 标题放在最后，允许里面出现竖线。

use std::fs;

use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use crate::model::{atomic_write, config_dir};

pub const WEEK_NAMES: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
pub const WEEK_FULL: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
/// bit0 = 周一 … bit6 = 周日。
pub const REPEAT_ALL: u8 = 0b111_1111;
pub const REPEAT_WORKDAYS: u8 = 0b001_1111;
pub const REPEAT_WEEKEND: u8 = 0b110_0000;
/// Windows 自带铃声；用户没选铃声时用它。
pub const DEFAULT_RING: &str = r"C:\Windows\Media\Alarm01.wav";
pub const SNOOZE_MIN: u32 = 1;
pub const SNOOZE_MAX: u32 = 60;

fn hhmm_to_min(text: &str) -> Option<u32> {
    let t = NaiveTime::parse_from_str(text.get(..5)?, "%H:%M").ok()?;
    Some(t.hour() * 60 + t.minute())
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

/// YYYY-MM-DD → 星期位（bit0 = 周一）。
pub fn weekday_bit(today: &str) -> u8 {
    parse_date(today).map(|d| 1u8 << d.weekday().num_days_from_monday()).unwrap_or(0)
}

/// 把 YYYY-MM-DD + HH:MM:SS 解析成时间点。
pub fn parse_now(today: &str, clock: &str) -> Option<NaiveDateTime> {
    let date = parse_date(today)?;
    let time = NaiveTime::parse_from_str(clock.get(..8).unwrap_or("00:00:00"), "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(clock.get(..5).unwrap_or("00:00"), "%H:%M"))
        .ok()?;
    Some(NaiveDateTime::new(date, time))
}

fn short_date(date: &str) -> String {
    match parse_date(date) {
        Some(d) => format!("{}月{}日", d.month(), d.day()),
        None => date.to_string(),
    }
}

#[derive(Clone, Debug)]
pub struct Alarm {
    pub label: String,
    pub hour: u8,
    pub minute: u8,
    /// bit0 = 周一 … bit6 = 周日；0 表示一次性（用 date）。
    pub repeat: u8,
    /// 一次性闹钟的日期 YYYY-MM-DD；重复闹钟忽略。
    pub date: String,
    pub enabled: bool,
    pub snooze_min: u32,
    /// 空 = 用设置里的默认铃声。
    pub ring: String,
    /// 上次响铃对应的「计划时刻」YYYY-MM-DD HH:MM，防止同一分钟重复响。
    pub last_fired: String,
    /// 贪睡到 YYYY-MM-DD HH:MM；空表示没有在贪睡。
    pub snooze_until: String,
}

impl Alarm {
    pub fn new(hour: u8, minute: u8, repeat: u8, date: &str, label: &str) -> Self {
        Self {
            label: label.to_string(),
            hour: hour.min(23),
            minute: minute.min(59),
            repeat: repeat & REPEAT_ALL,
            date: date.to_string(),
            enabled: true,
            snooze_min: 5,
            ring: String::new(),
            last_fired: String::new(),
            snooze_until: String::new(),
        }
    }

    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    pub fn is_repeating(&self) -> bool {
        self.repeat != 0
    }

    pub fn repeat_text(&self) -> String {
        let repeat = self.repeat & REPEAT_ALL;
        if repeat == 0 {
            return "仅一次".into();
        }
        if repeat == REPEAT_ALL {
            return "每天".into();
        }
        if repeat == REPEAT_WORKDAYS {
            return "工作日".into();
        }
        if repeat == REPEAT_WEEKEND {
            return "周末".into();
        }
        let days: Vec<&str> = (0..7usize).filter(|bit| repeat & (1 << bit) != 0).map(|bit| WEEK_NAMES[bit]).collect();
        format!("周{}", days.join(""))
    }

    /// 实际要播放的铃声文件。
    pub fn ring_file<'a>(&'a self, default: &'a str) -> &'a str {
        if self.ring.trim().is_empty() {
            default
        } else {
            self.ring.trim()
        }
    }

    /// 今天是否轮到它（不看时刻）。
    pub fn fires_on(&self, today: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.is_repeating() {
            self.repeat & weekday_bit(today) != 0
        } else {
            self.date == today
        }
    }

    fn target_key(&self, today: &str) -> String {
        format!("{today} {}", self.time_text())
    }

    /// 现在是否该响。容忍计划时刻后 1 分钟，避免机器休眠错过整分钟。
    pub fn should_ring(&self, today: &str, now_hm: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let now_key = format!("{today} {now_hm}");
        // 贪睡优先：到点就响，响过之后清空。
        if !self.snooze_until.is_empty() {
            return now_key.as_str() >= self.snooze_until.as_str() && self.last_fired != self.snooze_until;
        }
        if !self.fires_on(today) {
            return false;
        }
        let (Some(now), Some(target)) = (hhmm_to_min(now_hm), Some(self.hour as u32 * 60 + self.minute as u32)) else {
            return false;
        };
        let delta = (now + 24 * 60 - target) % (24 * 60);
        if delta > 1 {
            return false;
        }
        self.last_fired != self.target_key(today)
    }

    /// 「下次响铃」提示。
    pub fn next_text(&self, today: &str, now_hm: &str) -> String {
        if !self.enabled {
            return "已关闭".into();
        }
        let (Some(today_date), Some(now)) = (parse_date(today), hhmm_to_min(now_hm)) else {
            return String::new();
        };
        let target = self.hour as u32 * 60 + self.minute as u32;
        if !self.is_repeating() {
            let Some(date) = parse_date(&self.date) else { return "日期未设置".into() };
            if date < today_date {
                return "已过期".into();
            }
            if date == today_date && target <= now {
                return "今天已过".into();
            }
            return format!("{} {}", short_date(&self.date), self.time_text());
        }
        for offset in 0..8u64 {
            let day = today_date + Days::new(offset);
            let bit = 1u8 << day.weekday().num_days_from_monday();
            if self.repeat & bit == 0 {
                continue;
            }
            if offset == 0 && target <= now {
                continue;
            }
            let named = match offset {
                0 => "今天".to_string(),
                1 => "明天".to_string(),
                _ => WEEK_FULL[day.weekday().num_days_from_monday() as usize].to_string(),
            };
            return format!("{named} {}", self.time_text());
        }
        "未设置".into()
    }

    /// 下次响铃还有多少分钟；已经过去则返回负数或 None。
    pub fn minutes_until(&self, today: &str, now_hm: &str) -> Option<i64> {
        let now = hhmm_to_min(now_hm)? as i64;
        let Some(today_date) = parse_date(today) else { return None };
        let target = self.hour as i64 * 60 + self.minute as i64;
        if !self.is_repeating() {
            let date = parse_date(&self.date)?;
            let days = (date - today_date).num_days();
            return Some(days * 24 * 60 + target - now);
        }
        for offset in 0..8i64 {
            let day = today_date + Days::new(offset as u64);
            let bit = 1u8 << day.weekday().num_days_from_monday();
            if self.repeat & bit == 0 {
                continue;
            }
            let delta = offset * 24 * 60 + target - now;
            if delta > 0 {
                return Some(delta);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct AlarmList {
    pub items: Vec<Alarm>,
    /// 0..=100；转成 MCI 的 0..1000。
    pub volume: u8,
    /// 默认铃声；空 = DEFAULT_RING。
    pub ring_file: String,
    /// 新建闹钟默认贪睡分钟。
    pub snooze_min: u32,
    /// 响到手动停（否则只响一遍铃声）。
    pub loop_ring: bool,
    /// 列表滚动位置（下标）。
    pub scroll: usize,
}

impl Default for AlarmList {
    fn default() -> Self {
        Self::fresh()
    }
}

impl AlarmList {
    pub fn load() -> Self {
        fs::read_to_string(config_dir().join("alarms.txt")).map(|text| Self::parse(&text)).unwrap_or_else(|_| Self::fresh())
    }

    fn fresh() -> Self {
        Self { items: Vec::new(), volume: 80, ring_file: String::new(), snooze_min: 5, loop_ring: true, scroll: 0 }
    }

    pub fn default_ring(&self) -> &str {
        if self.ring_file.trim().is_empty() {
            DEFAULT_RING
        } else {
            self.ring_file.trim()
        }
    }

    pub fn parse(text: &str) -> Self {
        let mut list = Self::fresh();
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('@') {
                let mut kv = rest.splitn(2, '|');
                let key = kv.next().unwrap_or("");
                let value = kv.next().unwrap_or("");
                match key {
                    "volume" => list.volume = value.parse::<u8>().unwrap_or(80).min(100),
                    "ring" => list.ring_file = value.to_string(),
                    "snooze" => list.snooze_min = value.parse::<u32>().unwrap_or(5).clamp(SNOOZE_MIN, SNOOZE_MAX),
                    "loop" => list.loop_ring = value != "0",
                    _ => {}
                }
                continue;
            }
            let Some(rest) = line.strip_prefix("a|") else { continue };
            let fields: Vec<&str> = rest.splitn(9, '|').collect();
            if fields.len() < 9 {
                continue;
            }
            let parse_hm = |text: &str| -> (u8, u8) {
                match hhmm_to_min(text) {
                    Some(minutes) => ((minutes / 60) as u8, (minutes % 60) as u8),
                    None => (7, 0),
                }
            };
            let (hour, minute) = parse_hm(fields[1]);
            let Ok(repeat) = fields[2].parse::<u8>() else { continue };
            let mut alarm = Alarm::new(hour, minute, repeat, fields[3], fields[8].trim());
            alarm.enabled = fields[0] == "1";
            alarm.snooze_min = fields[4].parse::<u32>().unwrap_or(5).clamp(SNOOZE_MIN, SNOOZE_MAX);
            alarm.last_fired = fields[5].to_string();
            alarm.snooze_until = fields[6].to_string();
            alarm.ring = fields[7].to_string();
            list.items.push(alarm);
        }
        list
    }

    pub fn serialize(&self) -> String {
        let clean = |text: &str| text.replace(['\n', '\r'], " ");
        let mut out = String::new();
        out.push_str(&format!("@volume|{}\n", self.volume.min(100)));
        out.push_str(&format!("@ring|{}\n", clean(&self.ring_file)));
        out.push_str(&format!("@snooze|{}\n", self.snooze_min.clamp(SNOOZE_MIN, SNOOZE_MAX)));
        out.push_str(&format!("@loop|{}\n", if self.loop_ring { 1 } else { 0 }));
        for alarm in &self.items {
            out.push_str(&format!(
                "a|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                if alarm.enabled { 1 } else { 0 },
                alarm.time_text(),
                alarm.repeat & REPEAT_ALL,
                clean(&alarm.date),
                alarm.snooze_min.clamp(SNOOZE_MIN, SNOOZE_MAX),
                clean(&alarm.last_fired),
                clean(&alarm.snooze_until),
                clean(&alarm.ring),
                clean(&alarm.label),
            ));
        }
        out
    }

    pub fn save(&self) {
        let path = config_dir().join("alarms.txt");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = atomic_write(&path, self.serialize().as_bytes());
    }

    /// 找出该响的闹钟并记下「已经响过」，返回下标。
    pub fn take_due(&mut self, today: &str, now_hm: &str) -> Option<usize> {
        let index = self.items.iter().position(|alarm| alarm.should_ring(today, now_hm))?;
        {
            let alarm = &mut self.items[index];
            let fired = if alarm.snooze_until.is_empty() {
                alarm.target_key(today)
            } else {
                alarm.snooze_until.clone()
            };
            alarm.last_fired = fired;
            alarm.snooze_until.clear();
        }
        self.save();
        Some(index)
    }

    /// 贪睡：从现在起 minutes 分钟后再响。
    pub fn snooze(&mut self, index: usize, now: NaiveDateTime, minutes: u32) {
        let Some(alarm) = self.items.get_mut(index) else { return };
        let until = now + chrono::Duration::minutes(minutes.clamp(SNOOZE_MIN, SNOOZE_MAX) as i64);
        alarm.snooze_until = until.format("%Y-%m-%d %H:%M").to_string();
        self.save();
    }

    pub fn add(&mut self, alarm: Alarm) -> usize {
        self.items.push(alarm);
        self.save();
        self.items.len() - 1
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
            self.save();
        }
    }
}

/// 正在响的闹钟（不持久化）。
#[derive(Clone, Debug)]
pub struct Ringing {
    pub index: usize,
    pub label: String,
    pub time: String,
    /// 开始响的时刻（秒，界面动效用）。
    pub since: f64,
    pub snoozed: bool,
}

/// 编辑中的闹钟草稿。
#[derive(Clone, Debug)]
pub struct Draft {
    /// 编辑已有闹钟时的下标；新闹钟为 None。
    pub index: Option<usize>,
    pub label: String,
    pub hour: u8,
    pub minute: u8,
    pub repeat: u8,
    pub date: String,
    pub snooze_min: u32,
    pub ring: String,
    pub enabled: bool,
    /// 输入框是否已经抢过焦点（只抢一次，避免每帧打断输入）。
    pub focused: bool,
}

impl Draft {
    pub fn from_alarm(index: usize, alarm: &Alarm) -> Self {
        Self {
            index: Some(index),
            label: alarm.label.clone(),
            hour: alarm.hour,
            minute: alarm.minute,
            repeat: alarm.repeat,
            date: alarm.date.clone(),
            snooze_min: alarm.snooze_min,
            ring: alarm.ring.clone(),
            enabled: alarm.enabled,
            focused: false,
        }
    }

    pub fn to_alarm(&self, date_fallback: &str) -> Alarm {
        let date = if self.date.is_empty() { date_fallback } else { self.date.as_str() };
        let mut alarm = Alarm::new(self.hour, self.minute, self.repeat, date, &self.label);
        alarm.enabled = self.enabled;
        alarm.snooze_min = self.snooze_min.clamp(SNOOZE_MIN, SNOOZE_MAX);
        alarm.ring = self.ring.trim().to_string();
        alarm
    }

    /// 一次性闹钟：日期固定在给定日期。
    pub fn one_shot(today: &str, hour: u8, minute: u8) -> Self {
        Self {
            index: None,
            label: String::new(),
            hour,
            minute,
            repeat: 0,
            date: today.to_string(),
            snooze_min: 5,
            ring: String::new(),
            enabled: true,
            focused: false,
        }
    }
}

/// 一次性闹钟的日期：还没到就是今天，已经过了就顺延到明天。
pub fn one_shot_date(today: &str, clock: &str, hour: u8, minute: u8) -> String {
    let target = hour as u32 * 60 + minute as u32;
    let now = hhmm_to_min(clock).unwrap_or(0);
    if target > now {
        return today.to_string();
    }
    parse_date(today).map(|d| (d + Days::new(1)).format("%Y-%m-%d").to_string()).unwrap_or_else(|| today.to_string())
}

/// 「N 分钟后再提醒」的目标时间；跨天时日期自动顺延。
pub fn countdown_target(today: &str, clock: &str, minutes: u32) -> Option<(String, u8, u8)> {
    let now = parse_now(today, clock)?;
    let at = now + chrono::Duration::minutes(minutes as i64);
    Some((at.format("%Y-%m-%d").to_string(), at.hour() as u8, at.minute() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_bits_follow_monday_first() {
        // 2026-11-14 是周六。
        assert_eq!(weekday_bit("2026-11-14"), 1 << 5);
        assert_eq!(weekday_bit("2026-11-16"), 1 << 0);
        assert_eq!(weekday_bit("乱写"), 0);
    }

    #[test]
    fn repeat_text_covers_presets() {
        assert_eq!(Alarm::new(7, 0, REPEAT_ALL, "", "").repeat_text(), "每天");
        assert_eq!(Alarm::new(7, 0, REPEAT_WORKDAYS, "", "").repeat_text(), "工作日");
        assert_eq!(Alarm::new(7, 0, REPEAT_WEEKEND, "", "").repeat_text(), "周末");
        assert_eq!(Alarm::new(7, 0, 0b0010101, "", "").repeat_text(), "周一三五");
        assert_eq!(Alarm::new(7, 0, 0, "", "").repeat_text(), "仅一次");
    }

    #[test]
    fn workday_alarm_skips_weekend() {
        let alarm = Alarm::new(7, 30, REPEAT_WORKDAYS, "", "");
        assert!(alarm.should_ring("2026-11-16", "07:30")); // 周一
        assert!(!alarm.should_ring("2026-11-14", "07:30")); // 周六
    }

    #[test]
    fn ringing_is_idempotent_within_the_same_minute() {
        let mut alarm = Alarm::new(7, 30, REPEAT_ALL, "", "");
        assert!(alarm.should_ring("2026-11-16", "07:30"));
        alarm.last_fired = alarm.target_key("2026-11-16");
        assert!(!alarm.should_ring("2026-11-16", "07:30"));
        assert!(!alarm.should_ring("2026-11-16", "07:31"));
        // 第二天同一分钟照常响。
        assert!(alarm.should_ring("2026-11-17", "07:30"));
    }

    #[test]
    fn one_shot_fires_on_its_own_date_only() {
        let alarm = Alarm::new(21, 5, 0, "2026-12-01", "交作业");
        assert!(!alarm.should_ring("2026-11-14", "21:05"));
        assert!(alarm.should_ring("2026-12-01", "21:05"));
    }

    #[test]
    fn snooze_fires_after_the_requested_delay() {
        let mut list = AlarmList::fresh();
        let mut alarm = Alarm::new(7, 30, REPEAT_ALL, "", "");
        alarm.last_fired = "2026-11-16 07:30".into();
        alarm.snooze_until = "2026-11-16 07:35".into();
        list.items.push(alarm);
        assert_eq!(list.take_due("2026-11-16", "07:34"), None);
        assert_eq!(list.take_due("2026-11-16", "07:35"), Some(0));
        assert!(list.items[0].snooze_until.is_empty());
        assert_eq!(list.take_due("2026-11-16", "07:35"), None);
    }

    #[test]
    fn text_format_round_trips() {
        let mut list = AlarmList::fresh();
        list.volume = 55;
        list.ring_file = r"D:\铃声\a|b.mp3".into();
        list.snooze_min = 9;
        list.loop_ring = false;
        let mut alarm = Alarm::new(6, 5, REPEAT_WEEKEND, "", "起床 | 别赖床");
        alarm.enabled = false;
        alarm.ring = r"C:\x\y.wav".into();
        list.items.push(alarm);
        let back = AlarmList::parse(&list.serialize());
        assert_eq!(back.volume, 55);
        assert_eq!(back.ring_file, r"D:\铃声\a|b.mp3");
        assert_eq!(back.snooze_min, 9);
        assert!(!back.loop_ring);
        assert_eq!(back.items.len(), 1);
        assert_eq!(back.items[0].label, "起床 | 别赖床");
        assert_eq!(back.items[0].repeat_text(), "周末");
        assert!(!back.items[0].enabled);
    }

    #[test]
    fn next_text_names_today_and_tomorrow() {
        let alarm = Alarm::new(7, 30, REPEAT_ALL, "", "");
        assert_eq!(alarm.next_text("2026-11-16", "06:00"), "今天 07:30");
        assert_eq!(alarm.next_text("2026-11-16", "08:00"), "明天 07:30");
        let workday = Alarm::new(7, 30, REPEAT_WORKDAYS, "", "");
        // 周六早上：下一个工作日是周一。
        assert_eq!(workday.next_text("2026-11-14", "06:00"), "周一 07:30");
    }

    #[test]
    fn countdown_rolls_over_midnight() {
        let (date, hour, minute) = countdown_target("2026-11-14", "23:40:00", 30).unwrap();
        assert_eq!((date.as_str(), hour, minute), ("2026-11-15", 0, 10));
    }
}
