use crate::config;
use crate::client::{ApiResult, CardClient};
use eframe::egui::{self, Color32, Context, FontData, FontDefinitions, FontFamily, RichText};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

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

pub fn style_vue_b(ctx: &Context) {
    setup_cjk_font(ctx);
    // 尽量复用 `bsphp.Rust.user` 的配色：同一套绿色品牌色 + 浅色视觉主题。
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

fn renew_url_for_user(user: &str) -> String {
    let u = user.trim();
    if u.is_empty() {
        config::CARD_RENEW_URL_BASE.to_string()
    } else {
        format!(
            "{}&user={}",
            config::CARD_RENEW_URL_BASE,
            urlencoding::encode(u)
        )
    }
}

#[derive(Default)]
struct Job {
    busy_off: bool,
    panel_busy_off: bool,
    status: Option<String>,
    notice: Option<String>,
    ready: Option<bool>,
    alerts: Vec<(Option<i64>, String)>,
    open_panel: Option<(String, String)>,
    panel_detail: Option<String>,
    logout_close_panel: bool,
    vip_update: Option<String>,
}

fn nz(s: String, fb: &str) -> String {
    if s.trim().is_empty() {
        fb.to_string()
    } else {
        s
    }
}

fn alert_body(code: Option<i64>, data: impl AsRef<str>) -> String {
    let d = data.as_ref().trim();
    let d = if d.is_empty() { "（空）" } else { d };
    format!("code: {}\ndata: {d}", code.map_or("null".to_string(), |c| c.to_string()))
}

pub struct CardDemoApp {
    client: Arc<Mutex<CardClient>>,
    tx: Sender<Job>,
    rx: Receiver<Job>,
    booted: bool,
    ready: bool,
    notice: String,
    status_line: String,
    busy: bool,
    main_tab: usize,
    sub_tab: usize,
    card_id: String,
    card_pwd: String,
    machine: String,
    mc_ka: String,
    mc_pwd: String,
    show_panel: bool,
    logged_id: String,
    vip_exp: String,
    panel_aux_pwd: String,
    panel_detail: String,
    panel_busy: bool,
    alert_q: Vec<(String, String)>,
}

impl CardDemoApp {
    pub fn new(client: CardClient) -> Self {
        let (tx, rx) = mpsc::channel();
        let mc = crate::machine::get_machine_code();
        Self {
            client: Arc::new(Mutex::new(client)),
            tx,
            rx,
            booted: false,
            ready: false,
            notice: "加载中…".into(),
            status_line: "待操作".into(),
            busy: false,
            main_tab: 0,
            sub_tab: 0,
            card_id: String::new(),
            card_pwd: String::new(),
            machine: mc.clone(),
            mc_ka: String::new(),
            mc_pwd: String::new(),
            show_panel: false,
            logged_id: String::new(),
            vip_exp: "-".into(),
            panel_aux_pwd: String::new(),
            panel_detail: String::new(),
            panel_busy: false,
            alert_q: Vec::new(),
        }
    }

    fn spawn(&mut self, f: impl FnOnce(Arc<Mutex<CardClient>>) -> Job + Send + 'static) {
        self.busy = true;
        let c = self.client.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f(c));
        });
    }

    fn poll(&mut self) {
        while let Ok(j) = self.rx.try_recv() {
            if j.busy_off {
                self.busy = false;
            }
            if j.panel_busy_off {
                self.panel_busy = false;
            }
            if let Some(s) = j.status {
                self.status_line = s;
            }
            if let Some(n) = j.notice {
                self.notice = n;
            }
            if let Some(r) = j.ready {
                self.ready = r;
            }
            if let Some((id, exp)) = j.open_panel {
                self.logged_id = id;
                self.vip_exp = exp;
                self.show_panel = true;
            }
            if let Some(d) = j.panel_detail {
                self.panel_detail = d;
            }
            if let Some(v) = j.vip_update {
                self.vip_exp = v;
            }
            if j.logout_close_panel {
                self.show_panel = false;
                self.logged_id.clear();
                self.vip_exp = "-".into();
            }
            for (c, m) in j.alerts {
                let t = c.map(|x| format!("BSPHP (code={x})")).unwrap_or_else(|| "BSPHP".into());
                self.alert_q.push((t, alert_body(c, m)));
            }
        }
    }

    fn bootstrap(&mut self) {
        if self.booted {
            return;
        }
        self.booted = true;
        self.spawn(|c| {
            let mut j = Job {
                busy_off: true,
                ..Default::default()
            };
            let mut cl = c.lock().unwrap();
            match cl.bootstrap() {
                Err(_) => {
                    j.notice = Some(
                        "初始化失败（请检查网络、AppEn 地址与密钥是否与后台一致）".into(),
                    );
                    j.alerts.push((
                        None,
                        "初始化失败：连接或会话获取失败".into(),
                    ));
                    j.ready = Some(false);
                }
                Ok(()) => {
                    let n = cl.get_notice().message();
                    j.notice = Some(if n.is_empty() {
                        "暂无公告".into()
                    } else {
                        n
                    });
                    j.ready = Some(true);
                }
            }
            j
        });
    }
}

impl eframe::App for CardDemoApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.bootstrap();
        self.poll();

        if let Some((t, m)) = self.alert_q.first().cloned() {
            egui::Window::new(&t).collapsible(false).show(ctx, |ui| {
                ui.label(&m);
                if ui.button("确定").clicked() {
                    self.alert_q.remove(0);
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(246, 250, 248))
                .rounding(egui::Rounding::same(16.0))
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("BSPHP 卡模式演示")
                        .size(26.0)
                        .strong()
                        .color(Color32::from_rgb(53, 73, 94)),
                );
                if self.busy {
                    ui.label(RichText::new("处理中…").color(Color32::from_rgb(66, 184, 131)));
                }
            });
            egui::Frame::none()
                .fill(Color32::from_rgb(250, 253, 252))
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                ui.label(RichText::new("公告").strong());
                ui.label(&self.notice);
            });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (i, name) in ["制作卡密登陆模式", "一键注册机器码账号"]
                    .iter()
                    .enumerate()
                {
                    let sel = self.main_tab == i;
                    let txt = if sel {
                        // 选中态文字颜色与 `style_vue_b()` 中的 mint 主色对齐，避免“颜色冲突”。
                        RichText::new(*name)
                            .strong()
                            .color(Color32::from_rgb(27, 98, 70))
                    } else {
                        RichText::new(*name)
                    };
                    if ui.selectable_label(sel, txt).clicked() {
                        self.main_tab = i;
                    }
                }
            });
            ui.separator();
            match self.main_tab {
                0 => {
                    ui.group(|ui| {
                        ui.label(RichText::new("制作的卡密直接登录").strong());
                        ui.horizontal(|ui| {
                            ui.label("卡串：");
                            ui.add(egui::TextEdit::singleline(&mut self.card_id).desired_width(240.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("密码：");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.card_pwd).desired_width(240.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.button("验证使用").clicked() {
                                let id = self.card_id.trim().to_string();
                                if id.is_empty() {
                                    self.status_line = "请输入卡串".into();
                                } else {
                                    let pwd = self.card_pwd.clone();
                                    let c = self.client.clone();
                                    let tx = self.tx.clone();
                                    self.busy = true;
                                    std::thread::spawn(move || {
                                        let mut j = Job {
                                            busy_off: true,
                                            ..Default::default()
                                        };
                                        let mut cl = c.lock().unwrap();
                                        let r = cl.login_ic(&id, &pwd, None, None);
                                        let exp = cl.get_date_ic().message();
                                        let exp_d = if exp.is_empty() {
                                            "-".into()
                                        } else {
                                            exp
                                        };
                                        let msg = nz(r.message(), "验证失败");
                                        j.status = Some(msg.clone());
                                        let ok = r.code == Some(1081) || msg.contains("1081");
                                        if ok {
                                            j.open_panel = Some((id, exp_d));
                                            j.status = Some("验证成功，主控制面板已在新窗口打开".into());
                                        }
                                        let _ = tx.send(j);
                                    });
                                }
                            }
                            if ui.button("网络测试").clicked() {
                                let c = self.client.clone();
                                let tx = self.tx.clone();
                                self.busy = true;
                                std::thread::spawn(move || {
                                    let mut j = Job {
                                        busy_off: true,
                                        ..Default::default()
                                    };
                                    let ok = c.lock().unwrap().connect();
                                    j.status = Some(if ok {
                                        "网络连接正常".into()
                                    } else {
                                        "网络连接异常".into()
                                    });
                                    let _ = tx.send(j);
                                });
                            }
                            if ui.button("版本检测").clicked() {
                                let c = self.client.clone();
                                let tx = self.tx.clone();
                                self.busy = true;
                                std::thread::spawn(move || {
                                    let mut j = Job {
                                        busy_off: true,
                                        ..Default::default()
                                    };
                                    let v = c.lock().unwrap().get_version().message();
                                    j.status = Some(if v.is_empty() {
                                        "版本获取失败".into()
                                    } else {
                                        format!("当前版本：{v}")
                                    });
                                    let _ = tx.send(j);
                                });
                            }
                            if ui.button("续费充值").clicked() {
                                let _ = open::that(renew_url_for_user(&self.card_id));
                            }
                            if ui.button("购买充值卡").clicked() {
                                let _ = open::that(config::CARD_GEN_URL);
                            }
                            if ui.button("购买库存卡").clicked() {
                                let _ = open::that(config::CARD_STOCK_URL);
                            }
                        });
                        egui::Frame::none()
                        .fill(Color32::from_rgb(250, 253, 252))
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(egui::Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(&self.status_line).weak());
                            });
                    });
                }
                1 => {
                    ui.label(RichText::new("机器码直接注册做卡号模式（账号就是机器码）").weak());
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.sub_tab == 0, "机器码验证使用").clicked() {
                            self.sub_tab = 0;
                        }
                        if ui.selectable_label(self.sub_tab == 1, "机器码充值续费").clicked() {
                            self.sub_tab = 1;
                        }
                    });
                    match self.sub_tab {
                        0 => {
                            ui.horizontal(|ui| {
                                ui.label("机器码：");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.machine).desired_width(260.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                if ui.button("验证使用").clicked() {
                                    let mid = self.machine.trim().to_string();
                                    if mid.is_empty() {
                                        self.status_line = "【机器码】请输入机器码（账号）".into();
                                    } else {
                                        let g = crate::machine::get_machine_code();
                                        let c = self.client.clone();
                                        let tx = self.tx.clone();
                                        self.busy = true;
                                        std::thread::spawn(move || {
                                            let mut j = Job {
                                                busy_off: true,
                                                ..Default::default()
                                            };
                                            let mut cl = c.lock().unwrap();
                                            let feat = cl.add_card_features(&mid, &g, &g);
                                            let fm = nz(feat.message(), "（无 data）");
                                            let fc = feat.code;
                                            let feat_ok = matches!(fc, Some(1011 | 1081))
                                                || fm.contains("1081")
                                                || fm.contains("成功");
                                            j.status = Some(format!(
                                                "[AddCardFeatures.key.ic] code={fc:?} {fm}"
                                            ));
                                            if !feat_ok {
                                                let _ = tx.send(j);
                                                return;
                                            }
                                            let r = cl.login_ic(&mid, "", Some(&g), Some(&g));
                                            let exp = cl.get_date_ic().message();
                                            let exp_d = if exp.is_empty() {
                                                "-".into()
                                            } else {
                                                exp
                                            };
                                            let msg = nz(r.message(), "验证失败");
                                            j.status = Some(format!("[login.ic] {msg}"));
                                            let ok = r.code == Some(1081) || msg.contains("1081");
                                            if ok {
                                                j.open_panel = Some((mid, exp_d));
                                                j.status = Some(
                                                    "验证成功（机器码账号），主控制面板已在新窗口打开"
                                                        .into(),
                                                );
                                            }
                                            let _ = tx.send(j);
                                        });
                                    }
                                }
                                if ui.button("网络测试").clicked() {
                                    let c = self.client.clone();
                                    let tx = self.tx.clone();
                                    self.busy = true;
                                    std::thread::spawn(move || {
                                        let mut j = Job {
                                            busy_off: true,
                                            ..Default::default()
                                        };
                                        let ok = c.lock().unwrap().connect();
                                        j.status = Some(if ok {
                                            "网络连接正常".into()
                                        } else {
                                            "网络连接异常".into()
                                        });
                                        let _ = tx.send(j);
                                    });
                                }
                                if ui.button("版本检测").clicked() {
                                    let c = self.client.clone();
                                    let tx = self.tx.clone();
                                    self.busy = true;
                                    std::thread::spawn(move || {
                                        let mut j = Job {
                                            busy_off: true,
                                            ..Default::default()
                                        };
                                        let v = c.lock().unwrap().get_version().message();
                                        j.status = Some(if v.is_empty() {
                                            "版本获取失败".into()
                                        } else {
                                            format!("当前版本：{v}")
                                        });
                                        let _ = tx.send(j);
                                    });
                                }
                            });
                            ui.label(RichText::new(&self.status_line).weak());
                        }
                        1 => {
                            ui.horizontal(|ui| {
                                ui.label("机器码：");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.machine).desired_width(260.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("充值卡号：");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.mc_ka).desired_width(260.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("充值密码：");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.mc_pwd).desired_width(260.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                if ui.button("确认充值").clicked() {
                                    let ic = self.machine.trim().to_string();
                                    let ka = self.mc_ka.trim().to_string();
                                    if ic.is_empty() {
                                        self.status_line = "【机器码】请输入机器码（账号）".into();
                                    } else if ka.is_empty() {
                                        self.status_line = "请输入充值卡号".into();
                                    } else {
                                        let pwd = self.mc_pwd.clone();
                                        let c = self.client.clone();
                                        let tx = self.tx.clone();
                                        self.busy = true;
                                        std::thread::spawn(move || {
                                            let mut j = Job {
                                                busy_off: true,
                                                ..Default::default()
                                            };
                                            let r = c.lock().unwrap().recharge_card(&ic, &ka, &pwd);
                                            let msg = nz(r.message(), "（无 data）");
                                            j.status = Some(format!("[chong.ic] code={:?} {msg}", r.code));
                                            let _ = tx.send(j);
                                        });
                                    }
                                }
                                if ui.button("一键支付续费充值").clicked() {
                                    let _ = open::that(renew_url_for_user(&self.machine));
                                }
                                if ui.button("购买充值卡").clicked() {
                                    let _ = open::that(config::CARD_GEN_URL);
                                }
                                if ui.button("购买库存卡").clicked() {
                                    let _ = open::that(config::CARD_STOCK_URL);
                                }
                            });
                            ui.label(RichText::new(&self.status_line).weak());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            if self.busy {
                ui.label("处理中…");
            }
        });

        if self.show_panel {
            let id = self.logged_id.clone();
            let exp = self.vip_exp.clone();
            egui::Window::new("主控制面板")
                .default_size([640.0, 520.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("当前卡号：{id}")).weak());
                    ui.label(RichText::new(&exp).strong());
                    ui.horizontal(|ui| {
                        if ui.button("刷新到期").clicked() && !self.panel_busy {
                            let c = self.client.clone();
                            let tx = self.tx.clone();
                            self.panel_busy = true;
                            std::thread::spawn(move || {
                                let mut j = Job::default();
                                let r = c.lock().unwrap().get_date_ic();
                                let t = r.message();
                                if !t.trim().is_empty() {
                                    j.vip_update = Some(t);
                                }
                                j.panel_busy_off = true;
                                let _ = tx.send(j);
                            });
                        }
                        self.pbtn(ui, "登录状态", |cl| cl.get_login_info());
                        self.pbtn(ui, "心跳", |cl| cl.heartbeat());
                        self.pbtn(ui, "公告", |cl| cl.get_notice());
                    });
                    ui.horizontal(|ui| {
                        self.pbtn(ui, "服务器时间", |cl| cl.get_server_date());
                        self.pbtn(ui, "版本", |cl| cl.get_version());
                        self.pbtn(ui, "软件描述", |cl| cl.get_soft_info());
                        self.pbtn(ui, "预设URL", |cl| cl.get_preset_url());
                        self.pbtn(ui, "Web地址", |cl| cl.get_web_url());
                    });
                    ui.label(RichText::new("自定义配置模型").strong());
                    ui.horizontal_wrapped(|ui| {
                        self.pbtn(ui, "软件配置", |cl| cl.get_app_custom("myapp"));
                        self.pbtn(ui, "VIP配置", |cl| cl.get_app_custom("myvip"));
                        self.pbtn(ui, "登录配置", |cl| cl.get_app_custom("mylogin"));
                    });
                    ui.label(RichText::new("公共函数").strong());
                    ui.horizontal_wrapped(|ui| {
                        self.pbtn(ui, "全局配置", |cl| cl.get_global_info(None));
                        self.pbtn(ui, "逻辑A", |cl| cl.get_logic_a());
                        self.pbtn(ui, "逻辑B", |cl| cl.get_logic_b());
                        let lid = id.clone();
                        self.pbtn(ui, "激活查询", move |cl| cl.query_card(&lid));
                    });
                    if ui.button("卡信息示例").clicked() && !self.panel_busy {
                        let lid2 = id.clone();
                        let aux = self.panel_aux_pwd.clone();
                        self.run_panel("卡信息示例", move |cl| {
                            cl.get_card_info(&lid2, &aux, "UserName", None)
                        });
                    }
                    ui.separator();
                    ui.label(RichText::new("卡密（解绑、绑定本机、卡信息 等需要时填写）").weak());
                    ui.label("可选：卡密码");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.panel_aux_pwd)
                            .password(true)
                            .desired_width(280.0),
                    );
                    ui.horizontal(|ui| {
                        let lid3 = id.clone();
                        let pwd = self.panel_aux_pwd.clone();
                        let mc = crate::machine::get_machine_code();
                        if ui.button("绑定本机").clicked() && !self.panel_busy {
                            self.run_panel("绑定本机", move |cl| cl.bind_card(&mc, &lid3, &pwd));
                        }
                        let lid4 = id.clone();
                        let pwd2 = self.panel_aux_pwd.clone();
                        if ui.button("解除绑定").clicked() && !self.panel_busy {
                            self.run_panel("解除绑定", move |cl| cl.unbind_card(&lid4, &pwd2));
                        }
                    });
                    ui.label(RichText::new("后台页面").weak());
                    ui.horizontal(|ui| {
                        if ui.button("续费充值").clicked() {
                            let _ = open::that(renew_url_for_user(&id));
                        }
                        if ui.button("购买充值卡").clicked() {
                            let _ = open::that(config::CARD_GEN_URL);
                        }
                        if ui.button("购买库存卡").clicked() {
                            let _ = open::that(config::CARD_STOCK_URL);
                        }
                    });
                    if ui.button("注销并返回登录").clicked() && !self.panel_busy {
                        let c = self.client.clone();
                        let tx = self.tx.clone();
                        self.panel_busy = true;
                        std::thread::spawn(move || {
                            let mut j = Job::default();
                            c.lock().unwrap().logout();
                            j.logout_close_panel = true;
                            j.panel_busy_off = true;
                            let _ = tx.send(j);
                        });
                    }
                    ui.label(RichText::new("接口返回").weak());
                    egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        ui.monospace(&self.panel_detail);
                    });
                });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
}

impl CardDemoApp {
    fn pbtn<F>(&mut self, ui: &mut egui::Ui, label: &'static str, call: F)
    where
        F: FnOnce(&mut CardClient) -> ApiResult + Send + 'static,
    {
        if ui.button(label).clicked() && !self.panel_busy {
            self.run_panel(label, call);
        }
    }

    fn run_panel<F>(&mut self, label: &'static str, call: F)
    where
        F: FnOnce(&mut CardClient) -> ApiResult + Send + 'static,
    {
        let c = self.client.clone();
        let tx = self.tx.clone();
        self.panel_busy = true;
        std::thread::spawn(move || {
            let mut j = Job::default();
            let r = call(&mut c.lock().unwrap());
            let body = nz(r.message(), "（无 data 文本）");
            j.panel_detail = Some(format!("[{label}]\n{}", alert_body(r.code, body)));
            j.panel_busy_off = true;
            let _ = tx.send(j);
        });
    }
}
