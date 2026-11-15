//! 农历、二十四节气、农历/公历节日、法定节假日与干支生肖。
//!
//! 全部离线：农历月长、节气日期、法定节假日都是内置数据表（`lunar_data.rs`，
//! 由 sxtwl 与 chinese_calendar 生成），换算只是累加月长，不联网、不加依赖。
//!
//! 覆盖范围：公历 1900-01-31 ～ 2100-12-31 的农历；节假日只覆盖已公布年份
//! （国务院每年 11 月左右公布次年安排，届时重新生成 `HOLIDAYS` 即可）。

use chrono::{Datelike, NaiveDate};

use crate::lunar_data::{FIRST_LUNAR_YEAR, HOLIDAYS, LUNAR_YEARS, SOLAR_TERMS};

/// 二十四节气，索引与数据表一致（0=冬至，1=小寒，…，23=大雪）。
pub const TERM_NAMES: [&str; 24] = [
    "冬至", "小寒", "大寒", "立春", "雨水", "惊蛰", "春分", "清明", "谷雨", "立夏", "小满", "芒种",
    "夏至", "小暑", "大暑", "立秋", "处暑", "白露", "秋分", "寒露", "霜降", "立冬", "小雪", "大雪",
];

const MONTH_NAMES: [&str; 12] = [
    "正月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "冬月", "腊月",
];

const ZODIACS: [&str; 12] = [
    "鼠", "牛", "虎", "兔", "龙", "蛇", "马", "羊", "猴", "鸡", "狗", "猪",
];

const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
const BRANCHES: [&str; 12] = [
    "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
];

/// 1900 年正月初一（农历换算的起点）。
fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(FIRST_LUNAR_YEAR, 1, 31).expect("基准日期有效")
}

fn year_row(year: i32) -> Option<(u8, [u8; 12], u8)> {
    let index = year.checked_sub(FIRST_LUNAR_YEAR)?;
    let row = LUNAR_YEARS.get(usize::try_from(index).ok()?)?;
    Some(*row)
}

/// 农历日期。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lunar {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub leap: bool,
}

impl Lunar {
    /// 该农历月有多少天（用来判断是不是除夕）。
    pub fn month_days(&self) -> u32 {
        let Some((leap, months, leap_len)) = year_row(self.year) else {
            return 30;
        };
        let length = if self.leap {
            leap_len
        } else {
            months.get(self.month as usize - 1).copied().unwrap_or(30)
        };
        u32::from(length)
    }

    /// 「十月初七」「闰六月初一」。
    pub fn text(&self) -> String {
        let month = MONTH_NAMES.get(self.month as usize - 1).copied().unwrap_or("正月");
        format!(
            "{}{}{}",
            if self.leap { "闰" } else { "" },
            month,
            day_name(self.day)
        )
    }

    /// 农历节日（除夕按「该农历月最后一天」判定）。
    pub fn festival(&self) -> Option<&'static str> {
        if self.month == 12 && self.day == self.month_days() {
            return Some("除夕");
        }
        match (self.month, self.day) {
            (1, 1) => Some("春节"),
            (1, 15) => Some("元宵"),
            (2, 2) => Some("龙抬头"),
            (5, 5) => Some("端午"),
            (7, 7) => Some("七夕"),
            (8, 15) => Some("中秋"),
            (9, 9) => Some("重阳"),
            (12, 8) => Some("腊八"),
            _ => None,
        }
    }
}

/// 农历日名：初一 … 三十。
pub fn day_name(day: u32) -> &'static str {
    const NAMES: [&str; 30] = [
        "初一", "初二", "初三", "初四", "初五", "初六", "初七", "初八", "初九", "初十",
        "十一", "十二", "十三", "十四", "十五", "十六", "十七", "十八", "十九", "二十",
        "廿一", "廿二", "廿三", "廿四", "廿五", "廿六", "廿七", "廿八", "廿九", "三十",
    ];
    NAMES.get(day.saturating_sub(1) as usize).copied().unwrap_or("初一")
}

/// 公历转农历。超出数据表范围返回 None。
pub fn solar_to_lunar(date: NaiveDate) -> Option<Lunar> {
    let mut offset = (date - epoch()).num_days();
    if offset < 0 {
        return None;
    }
    let mut year = FIRST_LUNAR_YEAR;
    loop {
        let (_, months, leap_len) = year_row(year)?;
        let days: i64 = months.iter().map(|&m| i64::from(m)).sum::<i64>() + i64::from(leap_len);
        if offset < days {
            break;
        }
        offset -= days;
        year += 1;
    }
    let (leap, months, leap_len) = year_row(year)?;
    for (index, &length) in months.iter().enumerate() {
        let month = index as u32 + 1;
        if offset < i64::from(length) {
            return Some(Lunar { year, month, day: offset as u32 + 1, leap: false });
        }
        offset -= i64::from(length);
        if leap == month as u8 {
            if offset < i64::from(leap_len) {
                return Some(Lunar { year, month, day: offset as u32 + 1, leap: true });
            }
            offset -= i64::from(leap_len);
        }
    }
    None
}

/// 二十四节气：该日若是节气，返回名字（如「立春」）。
pub fn solar_term(date: NaiveDate) -> Option<&'static str> {
    let row = SOLAR_TERMS.get(usize::try_from(date.year().checked_sub(FIRST_LUNAR_YEAR)?).ok()?)?;
    let today = u16::from(date.month() as u16 * 100 + date.day() as u16);
    row.iter()
        .position(|&value| value == today)
        .map(|index| TERM_NAMES[index])
}

/// 公历节日。
pub fn solar_festival(date: NaiveDate) -> Option<&'static str> {
    match (date.month(), date.day()) {
        (1, 1) => Some("元旦"),
        (2, 14) => Some("情人节"),
        (3, 8) => Some("妇女节"),
        (5, 1) => Some("劳动节"),
        (5, 4) => Some("青年节"),
        (6, 1) => Some("儿童节"),
        (9, 10) => Some("教师节"),
        (10, 1) => Some("国庆节"),
        (12, 25) => Some("圣诞节"),
        _ => None,
    }
}

/// 法定节假日 / 调休上班。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rest,
    Work,
}

/// 该日的官方安排（休 / 班）与节日名。
pub fn official(date: NaiveDate) -> Option<(Kind, &'static str)> {
    let day = date.year() * 10000 + date.month() as i32 * 100 + date.day() as i32;
    HOLIDAYS
        .iter()
        .find(|(start, end, ..)| *start <= day && day <= *end)
        .map(|&(_, _, kind, name)| (if kind == 1 { Kind::Rest } else { Kind::Work }, name))
}

/// 法定放假的节日（国务院《全国年节及纪念日放假办法》里的那几个）。
pub fn statutory(name: &str) -> bool {
    matches!(name, "元旦" | "春节" | "除夕" | "清明" | "劳动节" | "端午" | "中秋" | "国庆节")
}

/// 当年有没有节假日数据（国务院每年才公布一次，表里只覆盖 2024–2026）。
pub fn year_covered(year: i32) -> bool {
    HOLIDAYS.iter().any(|&(from, ..)| from / 10000 == year)
}

/// 该日若是节日，返回 (节日名, 是否放假)。
///
/// 「放假」以当年的官方安排为准（只有法定节假日与调休会进表，普通周末不算）；
/// 官方安排还没覆盖到的年份退回「法定节日」判断，宁可标成不放假也不乱造假期。
pub fn festival_off(date: NaiveDate) -> Option<(&'static str, bool)> {
    let name = solar_festival(date)
        .or_else(|| solar_to_lunar(date).and_then(|lunar| lunar.festival()))
        .or_else(|| if solar_term(date) == Some("清明") { Some("清明") } else { None })?;
    let off = match official(date) {
        Some((Kind::Rest, _)) => true,
        Some((Kind::Work, _)) => false,
        None => !year_covered(date.year()) && statutory(name),
    };
    Some((name, off))
}

/// 干支纪年，如「丙午」。
pub fn ganzhi(year: i32) -> String {
    let cycle = (year - 4).rem_euclid(60) as usize;
    format!("{}{}", STEMS[cycle % 10], BRANCHES[cycle % 12])
}

/// 生肖，如「马」。
pub fn zodiac(year: i32) -> &'static str {
    ZODIACS[(year - 4).rem_euclid(12) as usize]
}

/// ISO 周数。
pub fn iso_week(date: NaiveDate) -> u32 {
    date.iso_week().week()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn spring_festival_lands_on_the_known_days() {
        for (y, m, d) in [
            (2020, 1, 25),
            (2021, 2, 12),
            (2022, 2, 1),
            (2023, 1, 22),
            (2024, 2, 10),
            (2025, 1, 29),
            (2026, 2, 17),
        ] {
            let lunar = solar_to_lunar(date(y, m, d)).expect("在数据范围内");
            assert_eq!((lunar.month, lunar.day), (1, 1), "{y} 春节");
            assert_eq!(lunar.festival(), Some("春节"));
        }
    }

    #[test]
    fn leap_month_and_day_names_match_the_almanac() {
        let leap = solar_to_lunar(date(2025, 7, 25)).unwrap();
        assert!(leap.leap);
        assert_eq!((leap.month, leap.day), (6, 1));
        assert_eq!(leap.text(), "闰六月初一");
        let today = solar_to_lunar(date(2026, 11, 15)).unwrap();
        assert_eq!(today.text(), "十月初七");
    }

    #[test]
    fn solar_terms_and_festivals() {
        assert_eq!(solar_term(date(2026, 2, 4)), Some("立春"));
        assert_eq!(solar_term(date(2026, 12, 22)), Some("冬至"));
        assert_eq!(solar_term(date(2026, 4, 5)), Some("清明"));
        assert_eq!(solar_term(date(2026, 11, 7)), Some("立冬"));
        assert_eq!(solar_term(date(2026, 11, 8)), None);
        assert_eq!(solar_festival(date(2026, 10, 1)), Some("国庆节"));
        assert_eq!(solar_to_lunar(date(2026, 9, 25)).unwrap().festival(), Some("中秋"));
    }

    #[test]
    fn official_holidays_and_makeup_workdays() {
        assert_eq!(official(date(2026, 10, 1)), Some((Kind::Rest, "国庆节")));
        assert_eq!(official(date(2026, 10, 10)), Some((Kind::Work, "调休上班")));
        assert_eq!(official(date(2026, 11, 15)), None, "普通周末不算法定节假日");
    }

    #[test]
    fn festival_off_tells_holidays_from_plain_festivals() {
        assert_eq!(festival_off(date(2026, 10, 1)), Some(("国庆节", true)));
        assert_eq!(festival_off(date(2026, 2, 17)), Some(("春节", true)));
        assert_eq!(festival_off(date(2026, 4, 5)), Some(("清明", true)), "清明是节气也是法定假日");
        assert_eq!(festival_off(date(2026, 9, 10)), Some(("教师节", false)));
        assert_eq!(festival_off(date(2026, 10, 18)), Some(("重阳", false)));
        assert_eq!(festival_off(date(2026, 12, 25)), Some(("圣诞节", false)));
        assert_eq!(festival_off(date(2026, 10, 5)), None, "假期中间那天不是节日");
        assert_eq!(festival_off(date(2024, 2, 9)), Some(("除夕", false)), "2024 的除夕不在法定假期里");
        assert_eq!(festival_off(date(2027, 10, 1)), Some(("国庆节", true)), "未覆盖年份退回法定节日判断");
        assert_eq!(festival_off(date(2027, 9, 10)), Some(("教师节", false)));
    }

    #[test]
    fn ganzhi_zodiac_and_weeks() {
        assert_eq!(ganzhi(2026), "丙午");
        assert_eq!(zodiac(2026), "马");
        assert_eq!(iso_week(date(2026, 11, 15)), 46);
    }
}
