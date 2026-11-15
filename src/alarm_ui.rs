//! 闹钟页、编辑弹层与响铃界面。
//!
//! 坐标同样是面板局部逻辑点（419×140），文字 y 是基线。

use crate::alarm::{Alarm, Draft, Ringing, SNOOZE_MAX, SNOOZE_MIN};
use crate::app::{App, Cmd, Env, ALARM_PAGE};
use crate::extras::{island_switch, setting_button, stepper};
use crate::paint::{self, capsule, Card, H};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Pos2, Rect};

/// 滚轮两列的横坐标（面板局部逻辑点）。
fn wheel_column(column: usize) -> f32 {
    if column == 0 { 206.0 } else { 290.0 }
}

/// 把「还有多少分钟」说成人话。
fn human_left(minutes: i64) -> String {
    let m = minutes.max(0);
    if m < 60 {
        format!("{m} 分钟后")
    } else if m < 24 * 60 {
        format!("{} 小时 {} 分钟后", m / 60, m % 60)
    } else {
        format!("{} 天后", m / (24 * 60))
    }
}

/// 小闹钟图标：表盘 + 两根指针。
fn clock_icon(c: &Card, x: f32, y: f32, r: f32, color: Color32) {
    c.ring(x, y, r, 1.3, color);
    c.path(&[(x, y - r * 0.55), (x, y), (x + r * 0.5, y + r * 0.22)], 1.2, color);
}

impl App {
    /// 闹钟页：左边下一个闹钟，右边开关列表。
    pub(crate) fn page_alarm(&mut self, c: &Card, env: &Env) {
        let now_hm = env.clock.get(..5).unwrap_or("").to_string();
        // 下一个还没到的闹钟；再记一个「最近刚过掉的」，过期条目不能谎报成没有闹钟。
        let mut next: Option<(i64, Alarm)> = None;
        let mut stale: Option<(i64, usize, Alarm)> = None;
        for (index, alarm) in self.alarms.items.iter().enumerate().filter(|(_, alarm)| alarm.enabled) {
            match alarm.minutes_until(&env.today, &now_hm) {
                Some(left) if left >= 0 => {
                    if next.as_ref().map_or(true, |(best, _)| left < *best) {
                        next = Some((left, Alarm::clone(alarm)));
                    }
                }
                Some(left) => {
                    if stale.as_ref().map_or(true, |(best, _, _)| left > *best) {
                        stale = Some((left, index, Alarm::clone(alarm)));
                    }
                }
                None => {}
            }
        }
        let next = next.map(|(_, alarm)| alarm);
        // 与待办页同一套骨架：大数字基线 61，副标题 78，胶囊 96 起。
        match (&next, &stale) {
            (Some(alarm), _) => {
                let w = c.text(42.0, 61.0, &alarm.time_text(), size::HERO, W::Bold, theme::WHITE, H::Left);
                c.text(42.0 + w + 5.0, 60.0, &alarm.repeat_text(), size::TINY, W::Semibold, theme::YELLOW, H::Left);
                let left = alarm.minutes_until(&env.today, &now_hm).unwrap_or(0).max(0);
                let when = alarm.next_text(&env.today, &now_hm);
                let sub = if left < 24 * 60 { format!("{} · {}", when, human_left(left)) } else { when };
                c.text(42.0, 78.0, &c.fit(&sub, size::SMALL, W::Regular, 146.0), size::SMALL, W::Regular, theme::GRAY, H::Left);
            }
            (None, Some((_, index, alarm))) => {
                // 有闹钟、还开着，但那个时刻已经过去了：如实说出来，并让点一下能改。
                let hit = c.hit(41.0, 40.0, 150.0, 44.0, "alarm-stale").on_hover_text("这条已经过了，点一下改时间");
                let color = if hit.hovered() { theme::GRAY } else { theme::DIM };
                c.text(42.0, 61.0, &alarm.time_text(), size::HERO, W::Bold, color, H::Left);
                let sub = format!("{} · 点一下改时间", alarm.next_text(&env.today, &now_hm));
                c.text(42.0, 78.0, &c.fit(&sub, size::SMALL, W::Regular, 146.0), size::SMALL, W::Regular, theme::DIM, H::Left);
                if hit.clicked() {
                    self.open_alarm_draft(Draft::from_alarm(*index, alarm));
                }
            }
            (None, None) => {
                // 空状态：占位符按「07:30」的真实排布画骨架条（四个数字位各一根短杠 + 冒号两个小点），
                // 整块宽度与大字时刻一致。直接用 "--:--" 画只有 50 逻辑宽，右端早早收住，
                // 左栏右边就塌出一大条空白，看着像整体偏左。
                let tw = c.text_w("07:30", size::HERO, W::Bold);
                let cw = c.text_w(":", size::HERO, W::Bold);
                let dw = (tw - cw) / 4.0;
                let bar = (dw * 0.72).max(6.0);
                let cy = 61.0 - size::HERO * 0.36;
                // 空状态没必要跟真时刻一样贴左边：整块在左栏（42..196）里居中，
                // 两块留白就对称了；有闹钟时数字照旧回到 42。
                let left = 42.0 + ((196.0 - 42.0 - tw) * 0.5).max(0.0);
                let mut x = left;
                for slot in 0..4 {
                    c.fill(x + (dw - bar) * 0.5, cy - 1.6, bar, 3.2, 1.6, theme::DIM);
                    x += dw;
                    if slot == 1 {
                        c.circle(x + cw * 0.5, cy - 4.0, 1.4, theme::DIM);
                        c.circle(x + cw * 0.5, cy + 4.0, 1.4, theme::DIM);
                        x += cw;
                    }
                }
                c.text(left + tw * 0.5, 78.0, "还没有闹钟", size::SMALL, W::Regular, theme::GRAY, H::Center);
            }
        }
        let (add, ac) = capsule(c, 41.0, 96.0, 74.0, 26.0, "alarm-add", theme::BTN, theme::BTN_HOVER);
        paint::plus(&ac, 56.0, 109.0, 6.2, theme::WHITE, Color32::BLACK);
        ac.text(67.0, 112.5, "加一个", size::BODY, W::Regular, theme::WHITE, H::Left);
        if add.clicked() {
            let (hour, minute) = next.as_ref().map(|alarm| (alarm.hour, alarm.minute)).unwrap_or((7, 30));
            self.open_alarm_draft(Draft::one_shot(hour, minute));
        }
        // 倒计时：最常见的时长，点一下就到点提醒。
        let (cd, cdc) = capsule(c, 121.0, 96.0, 68.0, 26.0, "alarm-countdown", theme::BTN, theme::BTN_HOVER);
        clock_icon(&cdc, 135.0, 109.0, 6.0, theme::WHITE);
        cdc.text(146.0, 112.5, "倒计时", size::BODY, W::Regular, theme::WHITE, H::Left);
        if cd.clicked() {
            self.alarm_countdown = true;
            self.alarm_pos = [0, 30];
            self.alarm_wheel = [0.0; 2];
            self.alarm_drag = [0.0; 2];
            self.alarm_focus = true;
        }

        c.fill(196.0, 31.0, 181.0, 93.0, 12.0, theme::CARD);
        let count = self.alarms.items.len();
        if count == 0 {
            paint::alarm_clock(c, 286.5, 64.0, 32.0, Color32::from_gray(150), Color32::from_rgb(52, 54, 58));
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
            let expired = matches!(alarm.minutes_until(&env.today, &now_hm), Some(left) if left < 0);
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
            let label = if expired {
                "已过期".to_string()
            } else if alarm.label.trim().is_empty() {
                tag.clone()
            } else {
                alarm.label.clone()
            };
            let label = c.fit(&label, size::TINY, W::Regular, 56.0);
            rc.text(292.0, y + 3.6, &label, size::TINY, W::Regular, if expired { theme::YELLOW } else { theme::GRAY }, H::Left);
            let suffix = if expired { " · 已过期，点一下改时间" } else { " · 点击修改" };
            r.clone().on_hover_text(if alarm.label.trim().is_empty() {
                format!("{} · {}{suffix}", alarm.time_text(), tag)
            } else {
                format!("{} · {} · {}{suffix}", alarm.label, alarm.time_text(), tag)
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

    /// 自检与演示用：不依赖鼠标事件直接打开新建闹钟弹层。
    pub fn demo_alarm_editor(&mut self, env: &Env) {
        self.open_alarm_draft(Draft::one_shot(7, 30));
    }

    /// 自检与演示用：直接打开倒计时弹层。
    pub fn demo_alarm_countdown(&mut self) {
        self.alarm_countdown = true;
        self.alarm_pos = [0, 30];
        self.alarm_wheel = [0.0; 2];
        self.alarm_drag = [0.0; 2];
        self.alarm_focus = true;
    }

    /// 自检与演示用：直接摆出响铃界面。
    pub fn demo_alarm_ring(&mut self) {
        let (label, time) = self
            .alarms
            .items
            .first()
            .map(|alarm| (alarm.label.clone(), alarm.time_text()))
            .unwrap_or_else(|| ("起床".to_string(), "07:30".to_string()));
        self.alarm_ring = Some(Ringing { index: 0, label, time, since: 0.0, ringing: true, retry_at: 0.0, snoozed: false });
        self.open = true;
    }

    fn open_alarm_draft(&mut self, mut draft: Draft) {
        if draft.snooze_min == 0 {
            draft.snooze_min = self.alarms.snooze_min.max(SNOOZE_MIN);
        }
        self.alarm_pos = [draft.hour as i64, draft.minute as i64];
        self.alarm_wheel = [0.0; 2];
        self.alarm_drag = [0.0; 2];
        self.alarm_focus = true;
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

    /// 编辑弹层：左边标题与重复（标题可空），右边时间滚轮。
    pub(crate) fn draw_alarm_editor(&mut self, c: &Card, env: &Env) {
        let Some(draft) = self.alarm_edit.clone() else { return };
        let editing = draft.index.is_some();
        let w = c.text(15.0, 25.0, "闹钟", size::TITLE, W::Bold, theme::WHITE, H::Left);
        c.text(15.0 + w + 2.0, 25.0, if editing { "编辑" } else { "新建" }, size::TITLE, W::Bold, theme::YELLOW, H::Left);

        // 左栏：标题（可空）+ 重复；时间在右卡片上用滚轮挑。
        c.fill(41.0, 36.0, 145.0, 20.0, 7.0, Color32::from_rgb(34, 34, 38));

        // 星期：全不选 = 一次性。
        c.text(41.0, 76.0, "重复", 10.5, W::Regular, theme::GRAY, H::Left);
        for day in 0..7usize {
            let on = draft.repeat & (1 << day) != 0;
            let x = 66.0 + day as f32 * 18.0;
            let (r, b) = capsule(c, x, 62.0, 16.0, 19.0, &format!("alarm-day-{day}"), if on { Color32::from_rgb(39, 94, 150) } else { theme::BTN }, theme::BTN_HOVER);
            b.centered_text(x + 8.0, 71.5, crate::alarm::WEEK_NAMES[day], 9.0, W::Semibold, if on { theme::WHITE } else { theme::GRAY }, H::Center);
            if r.on_hover_text(format!("每周{}", crate::alarm::WEEK_NAMES[day])).clicked() {
                if let Some(d) = self.alarm_edit.as_mut() {
                    d.repeat ^= 1 << day;
                }
            }
        }
        if setting_button(c, 41.0, 90.0, 62.0, "每天", draft.repeat == crate::alarm::REPEAT_ALL, "alarm-every-day") {
            let repeat = if draft.repeat == crate::alarm::REPEAT_ALL { 0 } else { crate::alarm::REPEAT_ALL };
            if let Some(d) = self.alarm_edit.as_mut() {
                d.repeat = repeat;
            }
        }
        if setting_button(c, 107.0, 90.0, 74.0, "工作日", draft.repeat == crate::alarm::REPEAT_WORKDAYS, "alarm-workdays") {
            let repeat = if draft.repeat == crate::alarm::REPEAT_WORKDAYS { 0 } else { crate::alarm::REPEAT_WORKDAYS };
            if let Some(d) = self.alarm_edit.as_mut() {
                d.repeat = repeat;
            }
        }

        // 右卡片：时间滚轮 —— 和待办提醒同一套交互（滚轮 / 拖动 / 点选），带滚动动画。
        for column in 0..2 {
            self.wheel_input(c, column, wheel_column(column), 78.0);
        }
        let hour = self.alarm_pos[0].rem_euclid(24) as u8;
        let minute = self.alarm_pos[1].rem_euclid(60) as u8;
        if let Some(d) = self.alarm_edit.as_mut() {
            d.hour = hour;
            d.minute = minute;
        }
        let day_hint = if draft.repeat == 0 {
            let now_hm = env.clock.get(..5).unwrap_or("");
            if format!("{hour:02}:{minute:02}").as_str() > now_hm { "今天" } else { "明天" }
        } else {
            ""
        };
        self.draw_wheel_card(c, "时间", day_hint);
        self.alarm_focus = false;
        if c.hit(228.0, 100.0, 64.0, 20.0, "alarm-cancel").clicked() {
            self.alarm_edit = None;
        }
        c.centered_text(260.0, 110.0, "取消", 10.0, W::Regular, theme::GRAY, H::Center);
        let (save, b) = capsule(c, 302.0, 100.0, 66.0, 20.0, "alarm-save", Color32::from_rgb(10, 132, 255), Color32::from_rgb(35, 150, 255));
        b.centered_text(335.0, 110.0, "完成", 10.0, W::Semibold, theme::WHITE, H::Center);
        if save.clicked() {
            self.save_alarm_draft(env);
        }

        // 右上角：试听 / 删除。
        if c.hit(296.0, 14.0, 44.0, 18.0, "alarm-preview").clicked() {
            let path = self.alarms.default_ring().to_string();
            self.alarm_player.start(&path, self.alarms.volume, false);
        }
        c.centered_text(318.0, 26.0, "试听", 10.0, W::Regular, theme::GRAY, H::Center);
        if let Some(index) = draft.index {
            if c.hit(344.0, 14.0, 44.0, 18.0, "alarm-editor-delete").clicked() {
                self.alarms.remove(index);
                self.alarm_edit = None;
                self.say("闹钟已删除");
                return;
            }
            c.centered_text(366.0, 26.0, "删除", 10.0, W::Regular, theme::GRAY, H::Center);
        }
    }

    pub(crate) fn alarm_editor_inputs(&mut self, ui: &mut egui::Ui, origin: Pos2, cmds: &mut Vec<Cmd>, env: &Env) {
        let mut first = false;
        {
            let Some(draft) = self.alarm_edit.as_mut() else { return };
            let input = ui.put(
                Rect::from_min_size(origin + egui::vec2(47.0, 39.0), egui::vec2(133.0, 15.0)),
                egui::TextEdit::singleline(&mut draft.label)
                    .vertical_align(egui::Align::Center)
                    .hint_text("闹钟名（可空）")
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
            crate::alarm::normalize_one_shot(&draft.date, draft.hour, draft.minute, &env.today, &env.clock)
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
        c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        let label = if !ring.ringing && self.alarms.retry_min > 0 {
            format!("铃声已停 · {} 分钟后再响", self.alarms.retry_min)
        } else if !ring.ringing {
            "铃声已停 · 点停止关掉".to_string()
        } else if ring.label.trim().is_empty() {
            "时间到了".to_string()
        } else {
            ring.label.clone()
        };
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
        self.settle_page();
        self.forced_open_until = 0.0;
        self.say(format!("{minutes} 分钟后再提醒"));
    }

    pub fn dismiss_alarm(&mut self) {
        let was_ringing = self.alarm_ring.take().is_some();
        if was_ringing {
            self.alarm_player.stop();
        }
        // 回到闹钟主页时把转场状态归零：以前这里会留下滑了一半的位移，
        // 界面就停在半截帧上（左栏被切掉、像卡死），现在无条件复位。
        self.settle_page();
        self.forced_open_until = 0.0;
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
                        ringing: true,
                        retry_at: 0.0,
                        snoozed: false,
                    };
                    let file = alarm.ring_file(self.alarms.default_ring()).to_string();
                    self.alarm_player.start(&file, self.alarms.volume, true);
                    self.alarm_ring = Some(ring);
                    self.open = true;
                    self.forced_open_until = self.t + 600.0;
                    // 直接落位到闹钟页：走转场的话，响铃头 0.5 秒会露出上一页的残影。
                    self.jump_page(ALARM_PAGE);
                }
            }
        }
        if let Some(mut ring) = self.alarm_ring.clone() {
            let len = self.alarms.ring_len.min(crate::alarm::RING_LEN_MAX) as f64;
            let retry = self.alarms.retry_min.min(crate::alarm::RETRY_MAX) as f64 * 60.0;
            if ring.ringing {
                // 到了设定时长：先安静，再等着下一轮。
                if len > 0.0 && self.t - ring.since >= len {
                    ring.ringing = false;
                    ring.retry_at = if retry > 0.0 { self.t + retry } else { f64::MAX };
                    self.alarm_player.stop();
                }
            } else if retry > 0.0 && self.t >= ring.retry_at {
                let file = self
                    .alarms
                    .items
                    .get(ring.index)
                    .map(|alarm| alarm.ring_file(self.alarms.default_ring()).to_string())
                    .unwrap_or_else(|| self.alarms.default_ring().to_string());
                self.alarm_player.start(&file, self.alarms.volume, true);
                ring.ringing = true;
                ring.since = self.t;
                ring.retry_at = 0.0;
            }
            // 响铃（含静音等待）期间不让岛自己收起来。
            let keep = if ring.ringing { 5.0 } else { 2.0 };
            self.forced_open_until = self.forced_open_until.max(self.t + keep);
            self.alarm_ring = Some(ring);
        }
        self.alarm_player.tick(self.t);
    }

    /// 时间滚轮的输入：滚轮、拖动、点上下两行，都累加进 `alarm_pos[column]`。
    fn wheel_input(&mut self, c: &Card, column: usize, x: f32, width: f32) {
        let ctx = c.ui.ctx();
        let response = c.hit_drag(x, 52.0, width, 52.0, ("alarm-wheel", column));
        if response.hovered() {
            let (delta, now) = ctx.input(|i| {
                let delta = i
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::MouseWheel { unit, delta, modifiers } if !modifiers.ctrl && !modifiers.command => {
                            Some(-delta.y * if *unit == egui::MouseWheelUnit::Point { 1.0 } else { 40.0 })
                        }
                        _ => None,
                    })
                    .sum::<f32>();
                (delta, i.time)
            });
            if delta != 0.0 {
                if now - self.alarm_wheel_at[column] > 0.25 || self.alarm_wheel[column] * delta < 0.0 {
                    self.alarm_wheel[column] = 0.0;
                }
                self.alarm_wheel_at[column] = now;
                self.alarm_wheel[column] += delta;
                let steps = (self.alarm_wheel[column] / 40.0).trunc() as i64;
                self.alarm_wheel[column] -= steps as f32 * 40.0;
                self.alarm_pos[column] += steps;
                ctx.input_mut(|i| {
                    i.raw_scroll_delta = egui::Vec2::ZERO;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                });
            }
        }
        if response.drag_started() {
            self.alarm_drag[column] = 0.0;
        }
        if response.dragged() {
            self.alarm_drag[column] -= response.drag_delta().y / c.s;
            let steps = (self.alarm_drag[column] / 20.0).trunc() as i64;
            self.alarm_drag[column] -= steps as f32 * 20.0;
            self.alarm_pos[column] += steps;
        }
        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let y = c.to_local(p).y;
                if y < 67.0 {
                    self.alarm_pos[column] -= 1;
                } else if y > 85.0 {
                    self.alarm_pos[column] += 1;
                }
            }
        }
        response.on_hover_text(if column == 0 { "小时 · 滚轮、上下拖动或点选调整" } else { "分钟 · 滚轮、上下拖动或点选调整" });
    }

    /// 时间滚轮的外观：标题、右上角提示、选中带、两个冒号点、三行数字（中间选中）。
    fn draw_wheel_card(&self, c: &Card, title: &str, hint: &str) {
        c.fill(196.0, 31.0, 181.0, 93.0, 12.0, theme::CARD);
        c.text(204.0, 45.0, title, 9.5, W::Regular, theme::GRAY, H::Left);
        if !hint.is_empty() {
            c.text(369.0, 45.0, hint, 9.5, W::Regular, theme::DIM, H::Right);
        }
        c.fill(202.0, 65.0, 169.0, 22.0, 7.0, Color32::from_rgb(42, 42, 46));
        for y in [72.0, 80.0] {
            c.circle(287.0, y, 0.9, theme::WHITE);
        }
        let ctx = c.ui.ctx();
        for column in 0..2 {
            let x = wheel_column(column);
            let value = ctx.animate_value_with_time(
                egui::Id::new(("alarm-wheel-value", column)),
                self.alarm_pos[column] as f32,
                if self.alarm_focus { 0.0 } else { 0.13 },
            );
            let radix = if column == 0 { 24 } else { 60 };
            let clipped = c.clipped(x, 52.0, 78.0, 45.0);
            for step in value.floor() as i64 - 1..=value.floor() as i64 + 2 {
                let distance = step as f32 - value;
                let strength = (1.0 - distance.abs() * 0.55).clamp(0.0, 1.0);
                let size = 10.0 + (17.0 - 10.0) * strength;
                let color = Color32::from_gray((80.0 + 175.0 * strength) as u8);
                let text = format!("{:02}", step.rem_euclid(radix));
                clipped.centered_text(x + 39.0, 76.0 + distance * 18.0, &text, size, if strength > 0.7 { W::Semibold } else { W::Regular }, color, H::Center);
            }
        }
    }

    /// 倒计时弹层：左边常用时长，右边自定义「时 / 分」。
    pub(crate) fn draw_countdown_editor(&mut self, c: &Card, env: &Env) {
        let w = c.text(15.0, 25.0, "倒计时", size::TITLE, W::Bold, theme::WHITE, H::Left);
        c.text(15.0 + w + 3.0, 26.0, "到点响铃", size::TINY, W::Regular, theme::GRAY, H::Left);
        c.text(41.0, 60.0, "常用时长", 10.5, W::Regular, theme::GRAY, H::Left);
        let total = self.alarm_pos[0].rem_euclid(24) * 60 + self.alarm_pos[1].rem_euclid(60);
        let presets = [("5 分钟", 5i64), ("15 分钟", 15), ("30 分钟", 30), ("1 小时", 60)];
        for (index, (label, minutes)) in presets.iter().enumerate() {
            let x = 41.0 + (index % 2) as f32 * 74.0;
            let y = 68.0 + (index / 2) as f32 * 24.0;
            if setting_button(c, x, y, 70.0, label, total == *minutes, &format!("countdown-preset-{minutes}")) {
                self.alarm_pos = [minutes / 60, minutes % 60];
            }
        }

        for column in 0..2 {
            self.wheel_input(c, column, wheel_column(column), 78.0);
        }
        let hour = self.alarm_pos[0].rem_euclid(24);
        let minute = self.alarm_pos[1].rem_euclid(60);
        self.draw_wheel_card(c, "时长", "也可自己滚时 / 分");
        self.alarm_focus = false;
        if c.hit(228.0, 100.0, 64.0, 20.0, "countdown-cancel").clicked() {
            self.alarm_countdown = false;
        }
        c.centered_text(260.0, 110.0, "取消", 10.0, W::Regular, theme::GRAY, H::Center);
        let minutes_total = (hour * 60 + minute) as u32;
        let (start, sb) = capsule(c, 302.0, 100.0, 66.0, 20.0, "countdown-start", Color32::from_rgb(10, 132, 255), Color32::from_rgb(35, 150, 255));
        sb.centered_text(335.0, 110.0, "开始", 10.0, W::Semibold, theme::WHITE, H::Center);
        if start.clicked() && minutes_total > 0 {
            self.alarm_countdown = false;
            self.add_countdown(env, minutes_total);
        }
    }
}
