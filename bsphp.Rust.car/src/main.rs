//! BSPHP 卡模式演示：制作卡密登录、机器码账号；成功后打开主控制面板。

mod app;
mod config;
mod client;
mod crypto;
mod encode;
mod machine;

use app::CardDemoApp;
use client::{CardClient, CardClientConfig};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 620.0])
            .with_title("BSPHP 卡模式演示"),
        ..Default::default()
    };
    eframe::run_native(
        "BSPHP 卡模式演示",
        native_options,
        Box::new(|cc| {
            app::style_vue_b(&cc.egui_ctx);
            let cfg = CardClientConfig {
                url: config::BSPHP_URL.to_string(),
                mutual_key: config::BSPHP_MUTUAL_KEY.to_string(),
                server_private_key: config::BSPHP_SERVER_PRIVATE_KEY.to_string(),
                client_public_key: config::BSPHP_CLIENT_PUBLIC_KEY.to_string(),
            };
            Ok(Box::new(CardDemoApp::new(CardClient::new(cfg))) as Box<dyn eframe::App>)
        }),
    )
}
