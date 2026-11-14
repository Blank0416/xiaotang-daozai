//! 闹钟页、编辑弹层与响铃界面。
//!
//! 坐标同样是面板局部逻辑点（419×140），文字 y 是基线。

use crate::alarm::{Alarm, Draft, Ringing, SNOOZE_MAX, SNOOZE_MIN};
use crate::app::{App, Cmd, Env, ALARM_PAGE};
use crate::extras::{island_switch, setting_button, stepper};
use crate::paint::{self, capsule, Card, H};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Pos2, Rect};

/// 小闹钟图标：表盘 + 两根指针。
fn clock_icon(c: &Card, x: f32, y: f32, r: f32, color: Color32) {
    c.ring(x, y, r, 1.3, color);
    c.path(&[(x, y - r * 0.55), (x, y), (x + r * 0.5, y + r * 0.22)], 1.2, color);
}

impl App {
    /// 闹钟页：左边下一个闹钟，右边开关列表。
    pub(crate) fn page_alarm(&mut self, c: &Card, env: &Env) {
        let now_hm = env.clock.get(..5).unwrap_or("").to_string();
        let next = self
            .alarms
            .items
            .iter()
            .filter(|alarm| alarm.enabled)
            .filter_map(|alarm| alarm.minutes_until(&env.today, &now_hm).map(|left| (left, Alarm::clone(alarm))))
            .filter(|(left, _)| *left >= 0)
            .min_by_key(|(left, _)| *left)
            .map(|(_, alarm)| alarm);
        // 与待办页同一套骨架：大数字基线 61，副标题 78，胶囊 96 起。
        match &next {
            Some(alarm) => {
                let w = c.text(42.0, 61.0, &alarm.time_text(), size::HERO, W::Bold, theme::WHITE, H::Left);
                c.text(42.0 + w + 5.0, 60.0, &alarm.repeat_text(), size::TINY, W::Semibold, theme::YELLOW, H::Left);
                let when = alarm.next_text(&env.today, &now_hm);
                c.text(42.0, 78.0, &c.fit(&when, size::SMALL, W::Regular, 146.0), size::SMALL, W::Regular, theme::GRAY, H::Left);
            }
            None => {
                c.text(42.0, 61.0, "--:--", size::HERO, W::Bold, theme::DIM, H::Left);
                c.text(42.0, 78.0, "还没有闹钟", size::SMALL, W::Regular, theme::GRAY, H::Left);
            }
        }
        let (add, ac) = capsule(c, 41.0, 96.0, 74.0, 26.0, "alarm-add", theme::BTN, theme::BTN_HOVER);
        paint::plus(&ac, 56.0, 109.0, 6.2, theme::WHITE, Color32::BLACK);
        ac.text(67.0, 112.5, "加一个", size::BODY, W::Regular, theme::WHITE, H::Left);
        if add.clicked() {
            let (hour, minute) = next.as_ref().map(|alarm| (alarm.hour, alarm.minute)).unwrap_or((7, 30));
            self.open_alarm_draft(Draft::one_shot(&env.today, hour, minute));
        }
        // 倒计时：最常见的时长，点一下就到点提醒。
        let (cd, cdc) = capsule(c, 121.0, 96.0, 68.0, 26.0, "alarm-countdown", theme::BTN, theme::BTN_HOVER);
        clock_icon(&cdc, 135.0, 109.0, 6.0, theme::WHITE);
        cdc.text(146.0, 112.5, "30 分", size::BODY, W::Regular, theme::WHITE, H::Left);
        if cd.clicked() {
            self.add_countdown(env, 30);
        }

        c.fill(196.0, 31.0, 181.0, 93.0, 12.0, theme::CARD);
        let count = self.alarms.items.len();
        if count == 0 {
            clock_icon(c, 286.5, 64.0, 30.0, Color32::from_gray(150));
            c.text(286.5, 99.0, "点「加一个」设闹钟", size::SMALL, W::Regular, theme::GRAY, H::Center);
            return;
        }
        let start = self.alarms.scroll.min(count.saturating_sub(1));
        let mut row = 0;
        let mut deleted = None;
        let mut edit = None;
        let mut toggled = None;
        for index in start..count {
            if row >= 3 {
                break;
            }
            let y = 46.0 + row as f32 * 22.0;
            let alarm = Alarm::clone(&self.alarms.items[index]);
            let rc = if alarm.enabled { c.clone() } else { c.faded(0.45) };
            if island_switch(c, 201.0, y - 10.0, alarm.enabled, &format!("alarm-switch-{index}")) {
                toggled = Some(index);
            }
            let r = c.hit(240.0, y - 10.0, 104.0, 20.0, ("alarm-row", index));
            let (h, _) = c.hover_t(&r);
            if h > 0.01 {
                c.fill(240.0, y - 10.0, 104.0, 20.0, 7.0, Color32::from_white_alpha((12.0 * h) as u8));
            }
            rc.text(243.0, y + 4.0, &alarm.time_text(), 11.5, W::Semibold, if alarm.enabled { theme::WHITE } else { theme::GRAY }, H::Left);
            let tag = alarm.repeat_text();
            let label = if alarm.label.trim().is_empty() { tag.clone() } else { alarm.label.clone() };
            let label = c.fit(&label, size::TINY, W::Regular, 56.0);
            rc.text(292.0, y + 3.6, &label, size::TINY, W::Regular, theme::GRAY, H::Left);
            r.clone().on_hover_text(if alarm.label.trim().is_empty() {
                format!("{} · {} · 点击修改", alarm.time_text(), tag)
            } else {
                format!("{} · {} · {}", alarm.label, alarm.time_text(), tag)
            });
            if r.clicked() {
                edit = Some(index);
            }
            let remove = c.hit(352.0, y - 9.0, 18.0, 18.0, ("alarm-delete", index)).on_hover_text("删除闹钟");
            paint::close(c, 361.0, y, 2.4, if remove.hovered() { theme::WHITE } else { theme::GRAY });
            if remove.clicked() {
                deleted = Some(index);
            }
            row += 1;
        }
        if let Some(index) = toggled {
            if let Some(alarm) = self.alarms.items.get_mut(index) {
                alarm.enabled = !alarm.enabled;
                alarm.snooze_until.clear();
            }
            self.alarms.save();
        }
        if let Some(index) = edit {
            if let Some(alarm) = self.alarms.items.get(index) {
                self.open_alarm_draft(Draft::from_alarm(index, alarm));
            }
        }
        if let Some(index) = deleted {
            self.alarms.remove(index);
            if self.alarm_edit.as_ref().is_some_and(|d| d.index == Some(index)) {
                self.alarm_edit = None;
            }
        }
        let rest = count.saturating_sub(start + row);
        if rest > 0 {
            let more = c.hit(330.0, 111.0, 42.0, 18.0, "alarm-more").on_hover_text("看后面的闹钟");
            c.text(370.0, 120.0, &format!("还有 {rest} 个 ›"), size::TINY, W::Regular, if more.hovered() { theme::WHITE } else { theme::DIM }, H::Right);
            if more.clicked() {
                self.alarms.scroll = start + row;
                if self.alarms.scroll >= count {
                    self.alarms.scroll = 0;
                }
            }
        } else {
            self.alarms.scroll = 0;
        }
    }

    fn open_alarm_draft(&mut self, mut draft: Draft) {
        if draft.snooze_min == 0 {
            draft.snooze_min = self.alarms.snooze_min.max(SNOOZE_MIN);
        }
        self.alarm_edit = Some(draft);
        self.subpage = None;
    }

    /// 倒计时：直接把「现在 + N 分钟」写成一次性闹钟。
    fn add_countdown(&mut self, env: &Env, minutes: u32) {
        let Some((date, hour, minute)) = crate::alarm::countdown_target(&env.today, &env.clock, minutes) else { return };
        let mut alarm = Alarm::new(hour, minute, 0, &date, "");
        alarm.snooze_min = self.alarms.snooze_min.max(SNOOZE_MIN);
        self.alarms.add(alarm);
        self.say(format!("{minutes} 分钟后提醒"));
        self.alarms.scroll = self.alarms.items.len().saturating_sub(1);
    }

    /// 编辑弹层：时间、重复、贪睡、标题。
    pub(crate) fn draw_alarm_editor(&mut self, c: &Card, env: &Env) {
        let Some(draft) = self.alarm_edit.clone() else { return };
        let editing = draft.index.is_some();
        let w = c.text(15.0, 25.0, "闹钟", size::TITLE, W::Bold, theme::WHITE, H::Left);
        c.text(15.0 + w + 2.0, 25.0, if editing { "编辑" } else { "新建" }, size::TITLE, W::Bold, theme::YELLOW, H::Left);

        // 时间行：时 / 分分开加减，贪睡放右边。
        let y = 40.0;
        c.text(43.0, y + 13.0, "时间", 10.5, W::Regular, theme::GRAY, H::Left);
        let dh = stepper(c, 178.0, y, &format!("{:02}", draft.hour), "alarm-hour");
        if dh != 0 {
            if let Some(d) = self.alarm_edit.as_mut() {
                d.hour = ((d.hour as i32 + dh).rem_euclid(24)) as u8;
            }
        }
        c.text(181.0, y + 13.0, ":", 11.0, W::Semibold, theme::WHITE, H::Left);
        let dm = stepper(c, 292.0, y, &format!("{:02}", draft.minute), "alarm-minute");
        if dm != 0 {
            if let Some(d) = self.alarm_edit.as_mut() {
                d.minute = ((d.minute as i32 + dm).rem_euclid(60)) as u8;
            }
        }
        let dsn = stepper(c, 370.0, y, &format!("{} 分", draft.snooze_min), "alarm-snooze");
        if dsn != 0 {
            if let Some(d) = self.alarm_edit.as_mut() {
                d.snooze_min = (d.snooze_min as i32 + dsn).clamp(SNOOZE_MIN as i32, SNOOZE_MAX as i32) as u32;
            }
        }

        // 星期：全不选 = 一次性。
        let y = 64.0;
        c.text(43.0, y + 13.0, "重复", 10.5, W::Regular, theme::GRAY, H::Left);
        for day in 0..7usize {
            let on = draft.repeat & (1 << day) != 0;
            let x = 91.0 + day as f32 * 26.0;
            let (r, b) = capsule(c, x, y, 23.0, 19.0, &format!("alarm-day-{day}"), if on { Color32::from_rgb(39, 94, 150) } else { theme::BTN }, theme::BTN_HOVER);
            b.centered_text(x + 11.5, y + 9.5, crate::alarm::WEEK_NAMES[day], 9.5, W::Semibold, if on { theme::WHITE } else { theme::GRAY }, H::Center);
            if r.on_hover_text(format!("每周{}", crate::alarm::WEEK_NAMES[day])).clicked() {
                if let Some(d) = self.alarm_edit.as_mut() {
                    d.repeat ^= 1 << day;
                }
            }
        }
        if setting_button(c, 280.0, y, 42.0, "每天", draft.repeat == crate::alarm::REPEAT_ALL, "alarm-every-day") {
            let repeat = if draft.repeat == crate::alarm::REPEAT_ALL { 0 } else { crate::alarm::REPEAT_ALL };
            if let Some(d) = self.alarm_edit.as_mut() {
                d.repeat = repeat;
            }
        }
        if setting_button(c, 326.0, y, 44.0, "工作日", draft.repeat == crate::alarm::REPEAT_WORKDAYS, "alarm-workdays") {
            let repeat = if draft.repeat == crate::alarm::REPEAT_WORKDAYS { 0 } else { crate::alarm::REPEAT_WORKDAYS };
            if let Some(d) = self.alarm_edit.as_mut() {
                d.repeat = repeat;
            }
        }

        // 标题
        let y = 88.0;
        c.text(43.0, y + 13.0, "标题", 10.5, W::Regular, theme::GRAY, H::Left);
        c.fill(91.0, y - 2.0, 279.0, 20.0, 7.0, Color32::from_rgb(34, 34, 38));
        // 文字由下面放置的 TextEdit 自己画（含 hint），这里只留底色，避免两层字叠在一起。

        let y = 108.0;
        if setting_button(c, 41.0, y, 64.0, "试听", false, "alarm-preview") {
            let path = self.alarms.default_ring().to_string();
            self.alarm_player.start(&path, self.alarms.volume, false);
        }
        if let Some(index) = draft.index {
            if setting_button(c, 111.0, y, 64.0, "删除", false, "alarm-editor-delete") {
                self.alarms.remove(index);
                self.alarm_edit = None;
                self.say("闹钟已删除");
                return;
            }
        }
        if setting_button(c, 222.0, y, 72.0, "取消", false, "alarm-cancel") {
            self.alarm_edit = None;
        }
        if setting_button(c, 300.0, y, 70.0, "保存", true, "alarm-save") {
            self.save_alarm_draft(env);
        }
    }

    pub(crate) fn alarm_editor_inputs(&mut self, ui: &mut egui::Ui, origin: Pos2, cmds: &mut Vec<Cmd>, env: &Env) {
        let mut first = false;
        {
            let Some(draft) = self.alarm_edit.as_mut() else { return };
            let input = ui.put(
                Rect::from_min_size(origin + egui::vec2(97.0, 88.0), egui::vec2(268.0, 18.0)),
                egui::TextEdit::singleline(&mut draft.label)
                    .vertical_align(egui::Align::Center)
                    .hint_text("起床 / 开会 / 吃药")
                    .id(egui::Id::new("alarm-label-edit"))
                    .frame(false)
                    .margin(egui::Margin::ZERO)
                    .text_color(theme::WHITE)
                    .font(theme::font(11.5, W::Regular)),
            );
            if !draft.focused {
                input.request_focus();
                draft.focused = true;
                first = true;
                cmds.push(Cmd::Focus);
            }
        }
        if !first && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.save_alarm_draft(env);
        }
    }

    fn save_alarm_draft(&mut self, env: &Env) {
        let Some(draft) = self.alarm_edit.take() else { return };
        let date = if draft.repeat == 0 {
            if draft.date.is_empty() {
                crate::alarm::one_shot_date(&env.today, &env.clock, draft.hour, draft.minute)
            } else {
                draft.date.clone()
            }
        } else {
            String::new()
        };
        let alarm = draft.to_alarm(&date);
        match draft.index {
            Some(index) => {
                if let Some(slot) = self.alarms.items.get_mut(index) {
                    let last_fired = slot.last_fired.clone();
                    *slot = alarm;
                    slot.last_fired = last_fired;
                }
            }
            None => {
                self.alarms.add(alarm);
                self.alarms.scroll = self.alarms.items.len().saturating_sub(1);
            }
        }
        self.alarms.save();
        self.say("闹钟已保存");
    }

    /// 响铃界面：盖住整张卡片，只剩「贪睡」和「停止」。
    pub(crate) fn draw_alarm_ring(&mut self, c: &Card, _env: &Env) {
        let Some(ring) = self.alarm_ring.clone() else { return };
        let e = (self.t - ring.since) as f32;
        let pulse = (e * 2.4).sin() * 0.5 + 0.5;
        let w = c.text(15.0, 25.0, "闹钟", size::TITLE, W::Bold, theme::WHITE, H::Left);
        c.text(15.0 + w + 2.0, 25.0, "时间到", size::TITLE, W::Bold, theme::YELLOW, H::Left);
        let glow = c.scaled(209.0, 62.0, 1.0 + pulse * 0.03);
        glow.text(209.0, 70.0, &ring.time, 34.0, W::Bold, theme::WHITE, H::Center);
        let label = if ring.label.trim().is_empty() { "时间到了".to_string() } else { ring.label.clone() };
        c.text(209.0, 88.0, &c.fit(&label, size::BODY, W::Regular, 300.0), size::BODY, W::Regular, theme::GRAY, H::Center);
        let snooze = self.alarms.items.get(ring.index).map(|alarm| alarm.snooze_min).unwrap_or(self.alarms.snooze_min);
        if setting_button(c, 96.0, 104.0, 106.0, &format!("贪睡 {snooze} 分"), false, "alarm-snooze-now") {
            self.snooze_alarm(snooze);
        }
        if setting_button(c, 216.0, 104.0, 106.0, "停止", true, "alarm-stop") {
            self.dismiss_alarm();
        }
    }

    fn snooze_alarm(&mut self, minutes: u32) {
        let Some(ring) = self.alarm_ring.take() else { return };
        if let Some(now) = self.alarm_now {
            self.alarms.snooze(ring.index, now, minutes);
        }
        self.alarm_player.stop();
        self.say(format!("{minutes} 分钟后再提醒"));
    }

    fn dismiss_alarm(&mut self) {
        if self.alarm_ring.take().is_some() {
            self.alarm_player.stop();
        }
    }

    /// 每帧调度：到点响铃、铃声续播。
    pub(crate) fn tick_alarms(&mut self, env: &Env) {
        self.alarm_now = crate::alarm::parse_now(&env.today, &env.clock);
        if !self.module_on(ALARM_PAGE) {
            if self.alarm_ring.is_some() {
                self.dismiss_alarm();
            }
            return;
        }
        let now_hm = env.clock.get(..5).unwrap_or("").to_string();
        if self.alarm_ring.is_none() {
            if let Some(index) = self.alarms.take_due(&env.today, &now_hm) {
                if let Some(alarm) = self.alarms.items.get(index) {
                    let ring = Ringing {
                        index,
                        label: alarm.label.clone(),
                        time: alarm.time_text(),
                        since: self.t,
                        snoozed: false,
                    };
                    let file = alarm.ring_file(self.alarms.default_ring()).to_string();
                    self.alarm_player.start(&file, self.alarms.volume, self.alarms.loop_ring);
                    self.alarm_ring = Some(ring);
                    self.open = true;
                    self.forced_open_until = self.t + 600.0;
                    self.go(ALARM_PAGE, 1.0);
                }
            }
        }
        if self.alarm_ring.is_some() {
            // 响铃期间不让岛自己收起来。
            self.forced_open_until = self.forced_open_until.max(self.t + 5.0);
        }
        self.alarm_player.tick(self.t);
    }
}
