use crate::config;
use crate::client::{
    AccountClient, ApiResult, CODE_TYPE_BACK_PWD, CODE_TYPE_LOGIN, CODE_TYPE_REG, CODE_TYPE_SAY,
    InitError, USER_INFO_FIELDS,
};
use eframe::egui::{
    self, Color32, Context, FontData, FontDefinitions, FontFamily, RichText, TextureHandle,
    TextureOptions,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

const TAB_NAMES: &[&str] = &[
    "密码登录",
    "短信登录",
    "邮箱登录",
    "账号注册",
    "短信注册",
    "邮箱注册",
    "解绑",
    "充值",
    "短信找回",
    "邮箱找回",
    "找回密码",
    "修改密码",
    "意见反馈",
];

fn setup_cjk_font(ctx: &Context) {
    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk_fallback".to_string(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk_fallback".to_string());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("cjk_fallback".to_string());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

pub fn style_vue_a(ctx: &Context) {
    setup_cjk_font(ctx);
    let mut v = egui::Visuals::light();
    let green = Color32::from_rgb(0x42, 0xb8, 0x83);
    let slate = Color32::from_rgb(0x35, 0x49, 0x5e);
    let panel = Color32::from_rgb(246, 250, 248);
    let soft = Color32::from_rgb(232, 241, 236);
    v.selection.bg_fill = green.linear_multiply(0.25);
    v.hyperlink_color = green;
    v.widgets.inactive.weak_bg_fill = soft;
    v.widgets.active.weak_bg_fill = green.linear_multiply(0.30);
    v.widgets.hovered.weak_bg_fill = green.linear_multiply(0.18);
    v.widgets.inactive.bg_stroke.color = Color32::from_rgb(206, 221, 214);
    v.widgets.hovered.bg_stroke.color = green.linear_multiply(0.7);
    v.widgets.active.bg_stroke.color = green;
    v.window_fill = panel;
    v.widgets.noninteractive.fg_stroke.color = slate;
    v.extreme_bg_color = Color32::from_rgb(236, 244, 240);
    v.window_rounding = egui::Rounding::same(14.0);
    v.menu_rounding = egui::Rounding::same(12.0);
    v.widgets.inactive.rounding = egui::Rounding::same(8.0);
    v.widgets.hovered.rounding = egui::Rounding::same(8.0);
    v.widgets.active.rounding = egui::Rounding::same(8.0);
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(24.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
}

#[derive(Default)]
struct JobDone {
    busy_off: bool,
    console_busy_off: bool,
    alerts: Vec<(Option<i64>, String)>,
    open_console: bool,
    notice: Option<String>,
    logged_in: Option<bool>,
    ready: Option<bool>,
    code_map: Option<HashMap<String, bool>>,
    captcha_rgba: Option<(Vec<u8>, u32, u32, u64)>,
    console_detail: Option<String>,
}

fn data_str(v: &Option<Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(x) if !x.is_null() => x.to_string(),
        _ => String::new(),
    }
}

fn alert_title(code: Option<i64>) -> String {
    code.map(|c| format!("BSPHP (code={c})"))
        .unwrap_or_else(|| "BSPHP".to_string())
}

fn alert_body(code: Option<i64>, data: impl AsRef<str>) -> String {
    let d = data.as_ref().trim();
    let d = if d.is_empty() { "（空）" } else { d };
    format!("code: {}\ndata: {d}", code.map_or("null".to_string(), |c| c.to_string()))
}

fn nz(s: String, fallback: &str) -> String {
    if s.trim().is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

fn timeout_lg_desc(code: Option<i64>) -> &'static str {
    match code {
        Some(5031) => "正常返回：在线状态有效",
        Some(5032) => "计点模式扣点失败",
        Some(5033) => "账号已冻结",
        Some(5030) => "账号已到期",
        Some(5026) => "登录超时",
        Some(1049) => "登录超时（已退出登录）",
        Some(5036) => "执行正常：客户端扣点API调用被动（后台状态码）",
        _ => "状态码未在 timeout.lg 说明表中",
    }
}

fn timeout_lg_message(r: &ApiResult) -> String {
    let data = nz(r.message(), "（无 data 文本）");
    format!("{}\n{}", timeout_lg_desc(r.code), data)
}

pub struct UserDemoApp {
    client: Arc<Mutex<AccountClient>>,
    tx: Sender<JobDone>,
    rx: Receiver<JobDone>,
    bootstrapped: bool,
    ready: bool,
    notice: String,
    code_on: HashMap<String, bool>,
    busy: bool,
    logged_in: bool,
    tab: usize,
    show_console: bool,
    console_detail: String,
    console_busy: bool,
    captcha_tex: Option<TextureHandle>,
    captcha_nonce: u64,
    alert_queue: VecDeque<(String, String)>,
    pl_user: String,
    pl_pass: String,
    pl_code: String,
    sl_mobile: String,
    sl_area: String,
    sl_coode: String,
    sl_sms: String,
    sl_key: String,
    sl_max: String,
    el_email: String,
    el_coode: String,
    el_code: String,
    el_key: String,
    el_max: String,
    rg_user: String,
    rg_pass: String,
    rg_pass2: String,
    rg_qq: String,
    rg_mail: String,
    rg_mobile: String,
    rg_q_idx: usize,
    rg_ans: String,
    rg_code: String,
    rg_ext: String,
    sr_mobile: String,
    sr_area: String,
    sr_coode: String,
    sr_sms: String,
    sr_user: String,
    sr_pwd: String,
    sr_pwdb: String,
    sr_key: String,
    er_email: String,
    er_coode: String,
    er_code: String,
    er_user: String,
    er_pwd: String,
    er_pwdb: String,
    er_key: String,
    ub_u: String,
    ub_p: String,
    py_u: String,
    py_p: String,
    py_ka: String,
    py_kp: String,
    py_verify: bool,
    rr_mobile: String,
    rr_area: String,
    rr_coode: String,
    rr_sms: String,
    rr_p1: String,
    rr_p2: String,
    er2_email: String,
    er2_coode: String,
    er2_code: String,
    er2_p1: String,
    er2_p2: String,
    bp_u: String,
    bp_q_idx: usize,
    bp_ans: String,
    bp_p1: String,
    bp_p2: String,
    bp_code: String,
    cp_u: String,
    cp_old: String,
    cp_n1: String,
    cp_n2: String,
    cp_img: String,
    fb_u: String,
    fb_p: String,
    fb_title: String,
    fb_contact: String,
    fb_type: usize,
    fb_body: String,
    fb_code: String,
}

const REG_Q: &[&str] = &[
    "你最喜欢的颜色？",
    "你母亲的名字？",
    "你父亲的名字？",
    "你的出生地？",
    "你最喜欢的食物？",
    "你的小学名称？",
    "自定义问题",
];

const FB_T: &[&str] = &["建议反馈", "BUG", "使用问题"];

impl UserDemoApp {
    pub fn new(client: AccountClient) -> Self {
        let (tx, rx) = mpsc::channel::<JobDone>();
        let mc = crate::machine::get_machine_code();
        Self {
            client: Arc::new(Mutex::new(client)),
            tx,
            rx,
            bootstrapped: false,
            ready: false,
            notice: String::from("加载中…"),
            code_on: HashMap::new(),
            busy: false,
            logged_in: false,
            tab: 0,
            show_console: false,
            console_detail: String::new(),
            console_busy: false,
            captcha_tex: None,
            captcha_nonce: 0,
            alert_queue: VecDeque::new(),
            pl_user: "admin".to_string(),
            pl_pass: "admin".to_string(),
            pl_code: String::new(),
            sl_mobile: String::new(),
            sl_area: String::from("86"),
            sl_coode: String::new(),
            sl_sms: String::new(),
            sl_key: mc.clone(),
            sl_max: mc.clone(),
            el_email: String::new(),
            el_coode: String::new(),
            el_code: String::new(),
            el_key: mc.clone(),
            el_max: mc.clone(),
            rg_user: String::new(),
            rg_pass: String::new(),
            rg_pass2: String::new(),
            rg_qq: String::new(),
            rg_mail: String::new(),
            rg_mobile: String::new(),
            rg_q_idx: 0,
            rg_ans: String::new(),
            rg_code: String::new(),
            rg_ext: String::new(),
            sr_mobile: String::new(),
            sr_area: String::from("86"),
            sr_coode: String::new(),
            sr_sms: String::new(),
            sr_user: String::new(),
            sr_pwd: String::new(),
            sr_pwdb: String::new(),
            sr_key: mc.clone(),
            er_email: String::new(),
            er_coode: String::new(),
            er_code: String::new(),
            er_user: String::new(),
            er_pwd: String::new(),
            er_pwdb: String::new(),
            er_key: mc.clone(),
            ub_u: String::new(),
            ub_p: String::new(),
            py_u: String::new(),
            py_p: String::new(),
            py_ka: String::new(),
            py_kp: String::new(),
            py_verify: true,
            rr_mobile: String::new(),
            rr_area: String::from("86"),
            rr_coode: String::new(),
            rr_sms: String::new(),
            rr_p1: String::new(),
            rr_p2: String::new(),
            er2_email: String::new(),
            er2_coode: String::new(),
            er2_code: String::new(),
            er2_p1: String::new(),
            er2_p2: String::new(),
            bp_u: String::new(),
            bp_q_idx: 0,
            bp_ans: String::new(),
            bp_p1: String::new(),
            bp_p2: String::new(),
            bp_code: String::new(),
            cp_u: String::new(),
            cp_old: String::new(),
            cp_n1: String::new(),
            cp_n2: String::new(),
            cp_img: String::new(),
            fb_u: String::new(),
            fb_p: String::new(),
            fb_title: String::new(),
            fb_contact: String::new(),
            fb_type: 0,
            fb_body: String::new(),
            fb_code: String::new(),
        }
    }

    fn is_code_on(&self, t: &str) -> bool {
        *self.code_on.get(t).unwrap_or(&true)
    }

    fn spawn_job(&mut self, f: impl FnOnce(Arc<Mutex<AccountClient>>) -> JobDone + Send + 'static) {
        self.busy = true;
        let c = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let done = f(c);
            let _ = tx.send(done);
        });
    }

    fn poll_jobs(&mut self, ctx: &Context) {
        while let Ok(j) = self.rx.try_recv() {
            if j.busy_off {
                self.busy = false;
            }
            if j.console_busy_off {
                self.console_busy = false;
            }
            if let Some(s) = j.console_detail {
                self.console_detail = s;
            }
            if let Some(n) = j.notice {
                self.notice = n;
            }
            if let Some(r) = j.ready {
                self.ready = r;
            }
            if let Some(m) = j.code_map {
                self.code_on = m;
            }
            if let Some(li) = j.logged_in {
                self.logged_in = li;
            }
            if j.open_console {
                self.show_console = true;
            }
            for (code, msg) in j.alerts {
                self.alert_queue
                    .push_back((alert_title(code), alert_body(code, msg)));
            }
            if let Some((rgba, w, h, nonce)) = j.captcha_rgba {
                let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let id = format!("cap_{nonce}");
                self.captcha_tex = Some(ctx.load_texture(id, ci, TextureOptions::LINEAR));
            }
        }
    }

    fn maybe_bootstrap(&mut self) {
        if self.bootstrapped {
            return;
        }
        self.bootstrapped = true;
        self.spawn_job(|c| {
            let mut done = JobDone::default();
            let mut client = c.lock().unwrap();
            let types: [&str; 4] = [
                CODE_TYPE_LOGIN,
                CODE_TYPE_REG,
                CODE_TYPE_BACK_PWD,
                CODE_TYPE_SAY,
            ];
            match client.bootstrap() {
                Err(e) => {
                    let msg = match e {
                        InitError::Connect => "连接失败".to_string(),
                        InitError::Sessl => "获取 BSphpSeSsL 失败".to_string(),
                    };
                    done.ready = Some(false);
                    done.notice = Some(msg.clone());
                    done.alerts.push((None, format!("初始化失败: {msg}")));
                }
                Ok(()) => {
                    let ce = client.get_code_enabled_types(&types);
                    let mut m = HashMap::new();
                    if let Some(Value::String(ds)) = &ce.data {
                        let parts: Vec<&str> = ds.split('|').collect();
                        for (i, t) in types.iter().enumerate() {
                            let on = parts
                                .get(i)
                                .map(|p| p.to_lowercase() == "checked")
                                .unwrap_or(true);
                            m.insert((*t).to_string(), on);
                        }
                    } else {
                        for t in types {
                            m.insert(t.to_string(), true);
                        }
                    }
                    let n = client.get_notice().message();
                    let notice = if n.is_empty() {
                        "公告获取失败".to_string()
                    } else {
                        n
                    };
                    done.code_map = Some(m);
                    done.notice = Some(notice);
                    done.ready = Some(true);
                }
            }
            done.busy_off = true;
            done
        });
    }

    fn fetch_captcha(&mut self) {
        if !self.ready {
            return;
        }
        let url = {
            let g = self.client.lock().unwrap();
            let u = g.code_image_url();
            if u.is_empty() {
                return;
            }
            format!("{u}&_={}", chrono::Local::now().timestamp_millis() as u64)
        };
        self.captcha_nonce = self.captcha_nonce.wrapping_add(1);
        let n = self.captcha_nonce;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut done = JobDone::default();
            if let Ok(resp) = reqwest::blocking::get(&url) {
                if resp.status() == 200 {
                    if let Ok(bytes) = resp.bytes() {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let img =
                                img.resize_exact(120, 36, image::imageops::FilterType::Lanczos3);
                            let rgba = img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            done.captcha_rgba = Some((rgba.into_raw(), w, h, n));
                            let _ = tx.send(done);
                            return;
                        }
                    }
                }
            }
            done.alerts.push((None, "验证码图片加载失败".into()));
            let _ = tx.send(done);
        });
    }

    fn row_edit(ui: &mut egui::Ui, label: &str, s: &mut String, secret: bool) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).monospace());
            let te = if secret {
                egui::TextEdit::singleline(s).password(true).desired_width(280.0)
            } else {
                egui::TextEdit::singleline(s).desired_width(280.0)
            };
            ui.add(te);
        });
    }

    /// 不持有 `&mut self`，避免与表单字段的可变借用冲突；若 `refresh` 为 true，调用方再执行 `fetch_captcha`。
    fn captcha_widgets(
        ui: &mut egui::Ui,
        code: &mut String,
        tex: &Option<TextureHandle>,
        refresh: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.label("验  证  码：");
            ui.add(egui::TextEdit::singleline(code).desired_width(120.0));
            if let Some(t) = tex {
                ui.image((t.id(), t.size_vec2()));
            }
            if ui.button("刷新").clicked() {
                *refresh = true;
            }
        });
    }
}

impl eframe::App for UserDemoApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.maybe_bootstrap();
        self.poll_jobs(ctx);

        if let Some((title, msg)) = self.alert_queue.front().cloned() {
            egui::Window::new(&title)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(&msg);
                    if ui.button("确定").clicked() {
                        self.alert_queue.pop_front();
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(242, 248, 245))
                .rounding(egui::Rounding::same(16.0))
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("BSPHP 账号模式演示").size(27.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(if self.ready { "在线" } else { "离线" })
                                    .color(if self.ready {
                                        Color32::from_rgb(21, 133, 79)
                                    } else {
                                        Color32::from_rgb(190, 98, 0)
                                    })
                                    .strong(),
                            );
                        });
                    });
                    ui.separator();
                    egui::Frame::none()
                        .fill(Color32::from_rgb(250, 253, 252))
                        .rounding(egui::Rounding::same(12.0))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("公告").strong());
                            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                                ui.label(&self.notice);
                            });
                        });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let dot = if self.ready { "●" } else { "●" };
                let col = if self.ready {
                    Color32::GREEN
                } else {
                    Color32::from_rgb(255, 165, 0)
                };
                ui.label(RichText::new(dot).color(col));
                let st = if self.ready {
                    "服务已连接"
                } else {
                    "服务未连接"
                };
                ui.label(RichText::new(st).strong());
                if self.busy {
                    ui.label("处理中…");
                }
                if self.logged_in {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("已登录").color(Color32::DARK_GREEN));
                    });
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                for (i, name) in TAB_NAMES.iter().enumerate() {
                    let sel = self.tab == i;
                    let txt = if sel {
                        RichText::new(*name).strong().color(Color32::from_rgb(27, 98, 70))
                    } else {
                        RichText::new(*name)
                    };
                    if ui.selectable_label(sel, txt).clicked() {
                        self.tab = i;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.tab {
                    0 => self.tab_password(ui, ctx),
                    1 => self.tab_sms_login(ui, ctx),
                    2 => self.tab_email_login(ui, ctx),
                    3 => self.tab_register(ui, ctx),
                    4 => self.tab_sms_register(ui, ctx),
                    5 => self.tab_email_register(ui, ctx),
                    6 => self.tab_unbind(ui),
                    7 => self.tab_recharge(ui),
                    8 => self.tab_sms_recover(ui, ctx),
                    9 => self.tab_email_recover(ui, ctx),
                    10 => self.tab_recover_pwd(ui, ctx),
                    11 => self.tab_change_pwd(ui),
                    12 => self.tab_feedback(ui, ctx),
                    _ => {}
                }
            });
                });
        });

        if self.show_console {
            egui::Window::new("控制台")
                .default_size([800.0, 520.0])
                .show(ctx, |ui| {
                    self.draw_console(ui, ctx);
                });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl UserDemoApp {
    fn tab_password(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "登录账号：", &mut self.pl_user, false);
        Self::row_edit(ui, "登录密码：", &mut self.pl_pass, true);
        let mut cap_ref = false;
        if self.is_code_on(CODE_TYPE_LOGIN) {
            Self::captcha_widgets(ui, &mut self.pl_code, &self.captcha_tex, &mut cap_ref);
        }
        if cap_ref {
            self.fetch_captcha();
        }
        ui.horizontal(|ui| {
            if ui.button("测试网络").clicked() {
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let ok = cc.lock().unwrap().connect();
                    d.alerts
                        .push((None, if ok { "测试连接成功!".into() } else { "测试连接失败!".into() }));
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("检测到期").clicked() {
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().get_end_time();
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，取到期时间失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("获取版本").clicked() {
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().get_version();
                    let txt = data_str(&r.data);
                    d.alerts.push((
                        r.code,
                        if txt.is_empty() {
                            "获取版本失败".into()
                        } else {
                            txt
                        },
                    ));
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("Web方式登陆").clicked() && self.ready {
                let url = {
                    let g = self.client.lock().unwrap();
                    format!("{}{}", config::BSPHP_WEB_LOGIN_URL, g.bs_php_sessl)
                };
                let _ = open::that(url);
                self.alert_queue.push_back((
                    "网页登录".into(),
                    "已在系统浏览器中打开登录页。完成登录后请点击「检测 Web 登录」。".into(),
                ));
            }
            if ui.button("检测 Web 登录").clicked() {
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().heartbeat();
                    if matches!(r.code, Some(5031 | 5036)) {
                        d.logged_in = Some(true);
                        d.open_console = true;
                        d.alerts.push((r.code, timeout_lg_message(&r)));
                    } else if matches!(r.code, Some(1049 | 5026 | 5030 | 5033)) {
                        d.logged_in = Some(false);
                        d.alerts.push((r.code, timeout_lg_message(&r)));
                    } else {
                        d.alerts.push((r.code, timeout_lg_message(&r)));
                    }
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("登录").clicked() {
                let u = self.pl_user.clone();
                let p = self.pl_pass.clone();
                let co = if self.is_code_on(CODE_TYPE_LOGIN) {
                    self.pl_code.clone()
                } else {
                    String::new()
                };
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().login(&u, &p, &co, "", "");
                    match r.code {
                        Some(1011 | 9908) => {
                            d.logged_in = Some(true);
                            d.open_console = true;
                            d.alerts.push((
                                r.code,
                                if r.code == Some(1011) {
                                    "登录成功！".into()
                                } else {
                                    "使用已经到期！".into()
                                },
                            ));
                        }
                        _ => d.alerts.push((
                            r.code,
                            nz(r.message(), "登录失败"),
                        )),
                    }
                    d.busy_off = true;
                    d
                });
            }
        });
    }

    fn tab_sms_login(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "手机号码：", &mut self.sl_mobile, false);
        Self::row_edit(ui, "区  号：", &mut self.sl_area, false);
        ui.horizontal(|ui| {
            ui.label("验  证  码：");
            ui.add(egui::TextEdit::singleline(&mut self.sl_coode).desired_width(120.0));
            if ui.button("发送验证码").clicked() {
                let m = self.sl_mobile.clone();
                let a = self.sl_area.clone();
                let co = self.sl_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_sms_code("login", &m, &a, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送短信验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.sl_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "短信验证码：", &mut self.sl_sms, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "绑定特征key：", &mut self.sl_key, false);
        Self::row_edit(ui, "maxoror：", &mut self.sl_max, false);
        if ui.button("短信登录").clicked() {
            let m = self.sl_mobile.clone();
            let a = self.sl_area.clone();
            let s = self.sl_sms.clone();
            let k = self.sl_key.clone();
            let x = self.sl_max.clone();
            let co = self.sl_coode.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().login_sms(&m, &a, &s, &k, &x, &co);
                match r.code {
                    Some(1011 | 9908) => {
                        d.logged_in = Some(true);
                        d.open_console = true;
                        d.alerts.push((
                            r.code,
                            if r.code == Some(1011) {
                                "登录成功！".into()
                            } else {
                                "使用已经到期！".into()
                            },
                        ));
                    }
                    _ => d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，短信验证码登录失败！"),
                    )),
                }
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_email_login(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "邮箱地址：", &mut self.el_email, false);
        ui.horizontal(|ui| {
            ui.label("验  证  码：");
            ui.add(egui::TextEdit::singleline(&mut self.el_coode).desired_width(120.0));
            if ui.button("发送验证码").clicked() {
                let e = self.el_email.clone();
                let co = self.el_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_email_code("login", &e, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送邮箱验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.el_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "邮箱验证码：", &mut self.el_code, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "绑定特征key：", &mut self.el_key, false);
        Self::row_edit(ui, "maxoror：", &mut self.el_max, false);
        if ui.button("邮箱登录").clicked() {
            let e = self.el_email.clone();
            let ec = self.el_code.clone();
            let k = self.el_key.clone();
            let x = self.el_max.clone();
            let co = self.el_coode.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().login_email(&e, &ec, &k, &x, &co);
                match r.code {
                    Some(1011 | 9908) => {
                        d.logged_in = Some(true);
                        d.open_console = true;
                        d.alerts.push((
                            r.code,
                            if r.code == Some(1011) {
                                "登录成功！".into()
                            } else {
                                "使用已经到期！".into()
                            },
                        ));
                    }
                    _ => d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，邮箱验证码登录失败！"),
                    )),
                }
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_register(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "注册账号：", &mut self.rg_user, false);
        ui.horizontal(|ui| {
            ui.label("注册密码：");
            ui.add(
                egui::TextEdit::singleline(&mut self.rg_pass)
                    .password(true)
                    .desired_width(140.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.rg_pass2)
                    .password(true)
                    .desired_width(140.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("QQ / 邮箱：");
            ui.add(egui::TextEdit::singleline(&mut self.rg_qq).desired_width(120.0));
            ui.add(egui::TextEdit::singleline(&mut self.rg_mail).desired_width(160.0));
        });
        Self::row_edit(ui, "手机号码：", &mut self.rg_mobile, false);
        ui.horizontal(|ui| {
            ui.label("密保问题：");
            egui::ComboBox::new(egui::Id::new("rg_q"), "")
                .selected_text(REG_Q[self.rg_q_idx.min(REG_Q.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, q) in REG_Q.iter().enumerate() {
                        ui.selectable_value(&mut self.rg_q_idx, i, *q);
                    }
                });
            ui.add(egui::TextEdit::singleline(&mut self.rg_ans).desired_width(160.0));
        });
        let mut cap_ref = false;
        if self.is_code_on(CODE_TYPE_REG) {
            Self::captcha_widgets(ui, &mut self.rg_code, &self.captcha_tex, &mut cap_ref);
        }
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "推  广  码：", &mut self.rg_ext, false);
        if ui.button("注册").clicked() {
            let q = REG_Q[self.rg_q_idx.min(REG_Q.len() - 1)].to_string();
            let co = if self.is_code_on(CODE_TYPE_REG) {
                self.rg_code.clone()
            } else {
                String::new()
            };
            let u = self.rg_user.clone();
            let p = self.rg_pass.clone();
            let p2 = self.rg_pass2.clone();
            let mob = self.rg_mobile.clone();
            let ans = self.rg_ans.clone();
            let qq = self.rg_qq.clone();
            let mail = self.rg_mail.clone();
            let ext = self.rg_ext.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().reg(
                    &u, &p, &p2, &co, &mob, &q, &ans, &qq, &mail, &ext,
                );
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，注册失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_sms_register(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "手机号码：", &mut self.sr_mobile, false);
        Self::row_edit(ui, "区  号：", &mut self.sr_area, false);
        ui.horizontal(|ui| {
            ui.label("验  证  码：");
            ui.add(egui::TextEdit::singleline(&mut self.sr_coode).desired_width(120.0));
            if ui.button("发送验证码").clicked() {
                let m = self.sr_mobile.clone();
                let a = self.sr_area.clone();
                let co = self.sr_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_sms_code("register", &m, &a, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送短信验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.sr_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "短信验证码：", &mut self.sr_sms, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "账号：", &mut self.sr_user, false);
        Self::row_edit(ui, "注册密码：", &mut self.sr_pwd, true);
        Self::row_edit(ui, "确认密码：", &mut self.sr_pwdb, true);
        Self::row_edit(ui, "绑定特征key：", &mut self.sr_key, false);
        if ui.button("短信注册").clicked() {
            let u = self.sr_user.clone();
            let m = self.sr_mobile.clone();
            let a = self.sr_area.clone();
            let sc = self.sr_sms.clone();
            let p = self.sr_pwd.clone();
            let p2 = self.sr_pwdb.clone();
            let k = self.sr_key.clone();
            let co = self.sr_coode.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc
                    .lock()
                    .unwrap()
                    .register_sms(&u, &m, &a, &sc, &p, &p2, &k, &co);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，短信验证码注册失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_email_register(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "邮箱地址：", &mut self.er_email, false);
        ui.horizontal(|ui| {
            ui.label("验  证  码：");
            ui.add(egui::TextEdit::singleline(&mut self.er_coode).desired_width(120.0));
            if ui.button("发送验证码").clicked() {
                let e = self.er_email.clone();
                let co = self.er_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_email_code("register", &e, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送邮箱验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.er_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "邮箱验证码：", &mut self.er_code, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "账号：", &mut self.er_user, false);
        Self::row_edit(ui, "注册密码：", &mut self.er_pwd, true);
        Self::row_edit(ui, "确认密码：", &mut self.er_pwdb, true);
        Self::row_edit(ui, "绑定特征key：", &mut self.er_key, false);
        if ui.button("邮箱注册").clicked() {
            let u = self.er_user.clone();
            let e = self.er_email.clone();
            let ec = self.er_code.clone();
            let p = self.er_pwd.clone();
            let p2 = self.er_pwdb.clone();
            let k = self.er_key.clone();
            let co = self.er_coode.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc
                    .lock()
                    .unwrap()
                    .register_email(&u, &e, &ec, &p, &p2, &k, &co);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，邮箱验证码注册失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_unbind(&mut self, ui: &mut egui::Ui) {
        Self::row_edit(ui, "登录账号：", &mut self.ub_u, false);
        Self::row_edit(ui, "登录密码：", &mut self.ub_p, true);
        if ui.button("解绑").clicked() {
            let u = self.ub_u.clone();
            let p = self.ub_p.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().unbind(&u, &p);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，解绑失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_recharge(&mut self, ui: &mut egui::Ui) {
        Self::row_edit(ui, "充值账号：", &mut self.py_u, false);
        Self::row_edit(ui, "登录密码：", &mut self.py_p, true);
        Self::row_edit(ui, "充值卡号：", &mut self.py_ka, false);
        Self::row_edit(ui, "充值密码：", &mut self.py_kp, true);
        ui.horizontal(|ui| {
            ui.label("是否需要验证密码：");
            ui.checkbox(&mut self.py_verify, "");
            ui.label(RichText::new("是(1) 验证登录密码，防止充值错误给了别人 / 否(0) 不验证").weak());
        });
        if ui.button("充值").clicked() {
            let u = self.py_u.clone();
            let p = self.py_p.clone();
            let v = self.py_verify;
            let ka = self.py_ka.clone();
            let kp = self.py_kp.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().pay(&u, &p, v, &ka, &kp);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，充值失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_sms_recover(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "手机号码：", &mut self.rr_mobile, false);
        Self::row_edit(ui, "区  号：", &mut self.rr_area, false);
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.rr_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "短信验证码：", &mut self.rr_sms, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "新密码：", &mut self.rr_p1, true);
        Self::row_edit(ui, "确认新密码：", &mut self.rr_p2, true);
        ui.horizontal(|ui| {
            if ui.button("发送验证码").clicked() {
                let m = self.rr_mobile.clone();
                let a = self.rr_area.clone();
                let co = self.rr_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_sms_code("reset", &m, &a, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送短信验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("短信找回").clicked() {
                let m = self.rr_mobile.clone();
                let a = self.rr_area.clone();
                let sc = self.rr_sms.clone();
                let p1 = self.rr_p1.clone();
                let p2 = self.rr_p2.clone();
                let co = self.rr_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc
                        .lock()
                        .unwrap()
                        .reset_sms_pwd(&m, &a, &sc, &p1, &p2, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，短信验证码找回失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
    }

    fn tab_email_recover(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "邮箱地址：", &mut self.er2_email, false);
        let mut cap_ref = false;
        Self::captcha_widgets(ui, &mut self.er2_coode, &self.captcha_tex, &mut cap_ref);
        if cap_ref {
            self.fetch_captcha();
        }
        Self::row_edit(ui, "邮箱验证码：", &mut self.er2_code, false);
        ui.label(RichText::new("OTP有效期：300秒").weak());
        Self::row_edit(ui, "新密码：", &mut self.er2_p1, true);
        Self::row_edit(ui, "确认新密码：", &mut self.er2_p2, true);
        ui.horizontal(|ui| {
            if ui.button("发送验证码").clicked() {
                let e = self.er2_email.clone();
                let co = self.er2_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc.lock().unwrap().send_email_code("reset", &e, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，发送邮箱验证码失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
            if ui.button("邮箱找回").clicked() {
                let e = self.er2_email.clone();
                let ec = self.er2_code.clone();
                let p1 = self.er2_p1.clone();
                let p2 = self.er2_p2.clone();
                let co = self.er2_coode.clone();
                self.spawn_job(move |cc| {
                    let mut d = JobDone::default();
                    let r = cc
                        .lock()
                        .unwrap()
                        .reset_email_pwd(&e, &ec, &p1, &p2, &co);
                    d.alerts.push((
                        r.code,
                        nz(r.message(), "系统错误，邮箱验证码找回失败！"),
                    ));
                    d.busy_off = true;
                    d
                });
            }
        });
    }

    fn tab_recover_pwd(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "登录账号：", &mut self.bp_u, false);
        ui.horizontal(|ui| {
            ui.label("密保问题：");
            egui::ComboBox::new(egui::Id::new("bp_q"), "")
                .selected_text(REG_Q[self.bp_q_idx.min(REG_Q.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, q) in REG_Q.iter().enumerate() {
                        ui.selectable_value(&mut self.bp_q_idx, i, *q);
                    }
                });
            ui.add(egui::TextEdit::singleline(&mut self.bp_ans).desired_width(160.0));
        });
        ui.horizontal(|ui| {
            ui.label("新密码：");
            ui.add(
                egui::TextEdit::singleline(&mut self.bp_p1)
                    .password(true)
                    .desired_width(140.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.bp_p2)
                    .password(true)
                    .desired_width(140.0),
            );
        });
        let mut cap_ref = false;
        if self.is_code_on(CODE_TYPE_BACK_PWD) {
            Self::captcha_widgets(ui, &mut self.bp_code, &self.captcha_tex, &mut cap_ref);
        }
        if cap_ref {
            self.fetch_captcha();
        }
        if ui.button("找回密码").clicked() {
            let q = REG_Q[self.bp_q_idx.min(REG_Q.len() - 1)].to_string();
            let co = if self.is_code_on(CODE_TYPE_BACK_PWD) {
                self.bp_code.clone()
            } else {
                String::new()
            };
            let u = self.bp_u.clone();
            let p1 = self.bp_p1.clone();
            let p2 = self.bp_p2.clone();
            let a = self.bp_ans.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().back_pass(&u, &p1, &p2, &q, &a, &co);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，找回密码失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_change_pwd(&mut self, ui: &mut egui::Ui) {
        Self::row_edit(ui, "登录账号：", &mut self.cp_u, false);
        Self::row_edit(ui, "旧密码：", &mut self.cp_old, true);
        ui.horizontal(|ui| {
            ui.label("新密码：");
            ui.add(
                egui::TextEdit::singleline(&mut self.cp_n1)
                    .password(true)
                    .desired_width(140.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.cp_n2)
                    .password(true)
                    .desired_width(140.0),
            );
        });
        if ui.button("修改密码").clicked() {
            let u = self.cp_u.clone();
            let o = self.cp_old.clone();
            let n1 = self.cp_n1.clone();
            let n2 = self.cp_n2.clone();
            let im = self.cp_img.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc.lock().unwrap().edit_pass(&u, &o, &n1, &n2, &im);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，修改密码失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn tab_feedback(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        Self::row_edit(ui, "账号：", &mut self.fb_u, false);
        Self::row_edit(ui, "密码：", &mut self.fb_p, true);
        Self::row_edit(ui, "标题：", &mut self.fb_title, false);
        Self::row_edit(ui, "联系：", &mut self.fb_contact, false);
        ui.horizontal(|ui| {
            ui.label("类型：");
            egui::ComboBox::new(egui::Id::new("fb_t"), "")
                .selected_text(FB_T[self.fb_type.min(FB_T.len() - 1)])
                .show_ui(ui, |ui| {
                    for (i, t) in FB_T.iter().enumerate() {
                        ui.selectable_value(&mut self.fb_type, i, *t);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("内容：");
            ui.add(egui::TextEdit::multiline(&mut self.fb_body).desired_width(320.0));
        });
        let mut cap_ref = false;
        if self.is_code_on(CODE_TYPE_SAY) {
            Self::captcha_widgets(ui, &mut self.fb_code, &self.captcha_tex, &mut cap_ref);
        }
        if cap_ref {
            self.fetch_captcha();
        }
        if ui.button("提交").clicked() {
            let leix = FB_T[self.fb_type.min(FB_T.len() - 1)].to_string();
            let co = if self.is_code_on(CODE_TYPE_SAY) {
                self.fb_code.clone()
            } else {
                String::new()
            };
            let u = self.fb_u.clone();
            let p = self.fb_p.clone();
            let ti = self.fb_title.clone();
            let ct = self.fb_contact.clone();
            let body = self.fb_body.clone();
            self.spawn_job(move |cc| {
                let mut d = JobDone::default();
                let r = cc
                    .lock()
                    .unwrap()
                    .feedback(&u, &p, &ti, &ct, &leix, &body, &co);
                d.alerts.push((
                    r.code,
                    nz(r.message(), "系统错误，意见反馈失败！"),
                ));
                d.busy_off = true;
                d
            });
        }
    }

    fn draw_console(&mut self, ui: &mut egui::Ui, _ctx: &Context) {
        if self.logged_in {
            ui.horizontal(|ui| {
                ui.label(RichText::new("已登录").color(Color32::GREEN));
            });
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(RichText::new("公用接口").strong());
            ui.horizontal_wrapped(|ui| {
                self.console_btn(ui, "服务器时间", |c| c.get_server_date());
                self.console_btn(ui, "预设URL", |c| c.get_preset_url());
                self.console_btn(ui, "Web地址", |c| c.get_web_url());
                self.console_btn(ui, "全局配置", |c| c.get_global_info());
            });
            ui.horizontal_wrapped(|ui| {
                self.console_btn(ui, "验证码开关(全部)", |c| {
                    c.get_code_enabled_types(&[
                        CODE_TYPE_LOGIN,
                        CODE_TYPE_REG,
                        CODE_TYPE_BACK_PWD,
                        CODE_TYPE_SAY,
                    ])
                });
                self.console_btn(ui, "登录验证码", |c| c.get_code_enabled_single(CODE_TYPE_LOGIN));
                self.console_btn(ui, "注册验证码", |c| c.get_code_enabled_single(CODE_TYPE_REG));
                self.console_btn(ui, "找回密码验证码", |c| {
                    c.get_code_enabled_single(CODE_TYPE_BACK_PWD)
                });
                self.console_btn(ui, "留言验证码", |c| c.get_code_enabled_single(CODE_TYPE_SAY));
            });
            ui.horizontal_wrapped(|ui| {
                self.console_btn(ui, "逻辑值A", |c| c.get_logic_a());
                self.console_btn(ui, "逻辑值B", |c| c.get_logic_b());
                self.console_btn(ui, "逻辑值A内容", |c| c.get_logic_info_a());
                self.console_btn(ui, "逻辑值B内容", |c| c.get_logic_info_b());
            });
            ui.label(RichText::new("自定义配置模型").strong());
            ui.horizontal_wrapped(|ui| {
                self.console_btn(ui, "软件配置", |c| c.get_app_custom("myapp"));
                self.console_btn(ui, "VIP配置", |c| c.get_app_custom("myvip"));
                self.console_btn(ui, "登录配置", |c| c.get_app_custom("mylogin"));
            });
            ui.label(RichText::new("通用接口").strong());
            ui.horizontal_wrapped(|ui| {
                self.console_btn(ui, "获取版本", |c| c.get_version());
                self.console_btn(ui, "获取软件描述", |c| c.get_soft_info());
            });
            ui.label(RichText::new("登录模式接口").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("注销登陆").clicked() && !self.console_busy {
                    let c = self.client.clone();
                    let tx = self.tx.clone();
                    self.console_busy = true;
                    std::thread::spawn(move || {
                        let mut d = JobDone::default();
                        let r = c.lock().unwrap().logout();
                        d.alerts.push((r.code, r.message()));
                        d.logged_in = Some(false);
                        d.console_busy_off = true;
                        let _ = tx.send(d);
                    });
                }
                if ui.button("检测到期").clicked() && !self.console_busy {
                    let c = self.client.clone();
                    let tx = self.tx.clone();
                    self.console_busy = true;
                    std::thread::spawn(move || {
                        let mut d = JobDone::default();
                        let mut cl = c.lock().unwrap();
                        let u = cl.get_user_info(Some("UserVipDate"));
                        let mut msg = u.message();
                        if let Some(i) = msg.find('=') {
                            msg = msg[i + 1..].trim().to_string();
                        }
                        if !msg.is_empty() {
                            d.alerts
                                .push((u.code, format!("到期时间：{msg}")));
                        } else {
                            let r = cl.get_end_time();
                            let t = r.message().trim().to_string();
                            if !t.is_empty() {
                                d.alerts.push((r.code, format!("到期时间：{t}")));
                            } else {
                                d.alerts.push((
                                    r.code,
                                    "系统错误，取到期时间失败！".into(),
                                ));
                            }
                        }
                        d.console_busy_off = true;
                        let _ = tx.send(d);
                    });
                }
                self.console_btn(ui, "取用户信息(默认)", |c| c.get_user_info(None));
                if ui.button("心跳包更新").clicked() && !self.console_busy {
                    let c = self.client.clone();
                    let tx = self.tx.clone();
                    self.console_busy = true;
                    std::thread::spawn(move || {
                        let mut d = JobDone::default();
                        let r = c.lock().unwrap().heartbeat();
                        d.console_detail =
                            Some(format!("[心跳包更新]\n{}", alert_body(r.code, timeout_lg_message(&r))));
                        d.console_busy_off = true;
                        let _ = tx.send(d);
                    });
                }
                self.console_btn(ui, "用户特征Key", |c| c.get_user_key());
            });
            ui.label(RichText::new("取用户信息 info 字段").weak());
            ui.horizontal_wrapped(|ui| {
                for (key, name) in USER_INFO_FIELDS {
                    let k = *key;
                    self.console_btn(ui, *name, move |c| c.get_user_info(Some(k)));
                }
            });
            ui.label(RichText::new("续费订阅推广").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("续费订阅(直接)").clicked() {
                    let user = {
                        let mut cl = self.client.lock().unwrap();
                        let r = cl.get_user_info(Some("UserName"));
                        let mut msg = r.message();
                        if let Some(i) = msg.find('=') {
                            msg = msg[i + 1..].trim().to_string();
                        } else {
                            msg = msg.trim().to_string();
                        }
                        msg
                    };
                    let mut url = config::BSPHP_RENEW_URL.to_string();
                    if !user.is_empty() {
                        url.push_str("&user=");
                        url.push_str(&urlencoding::encode(&user));
                    }
                    let _ = open::that(url);
                }
                if ui.button("购买充值卡").clicked() {
                    let _ = open::that(config::BSPHP_RENEW_CARD_URL);
                }
                if ui.button("购买库存卡").clicked() {
                    let _ = open::that(config::BSPHP_RENEW_STOCK_CARD_URL);
                }
            });
        });
        ui.separator();
        ui.label(RichText::new("接口返回").weak());
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            ui.monospace(&self.console_detail);
        });
    }

    fn console_btn<F>(&mut self, ui: &mut egui::Ui, label: &'static str, call: F)
    where
        F: FnOnce(&mut AccountClient) -> ApiResult + Send + 'static,
    {
        if ui.button(label).clicked() && !self.console_busy {
            let c = self.client.clone();
            let tx = self.tx.clone();
            self.console_busy = true;
            std::thread::spawn(move || {
                let mut d = JobDone::default();
                let r = call(&mut c.lock().unwrap());
                let body = nz(r.message(), "（无 data 文本）");
                d.console_detail = Some(format!("[{label}]\n{}", alert_body(r.code, body)));
                d.console_busy_off = true;
                let _ = tx.send(d);
            });
        }
    }
}
