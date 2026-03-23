//! BSPHP 账号模式演示（公告、多 Tab、控制台、网页登录）。

mod app;
mod config;
mod client;
mod crypto;
mod encode;
mod machine;

use app::UserDemoApp;
use client::{AccountClient, AccountClientConfig};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 860.0])
            .with_title("BSPHP 账号模式演示"),
        ..Default::default()
    };
    eframe::run_native(
        "BSPHP 账号模式演示",
        native_options,
        Box::new(|cc| {
            app::style_vue_a(&cc.egui_ctx);
            let cfg = AccountClientConfig {
                url: config::BSPHP_URL.to_string(),
                mutual_key: config::BSPHP_MUTUAL_KEY.to_string(),
                server_private_key: config::BSPHP_SERVER_PRIVATE_KEY.to_string(),
                client_public_key: config::BSPHP_CLIENT_PUBLIC_KEY.to_string(),
                code_url_prefix: config::BSPHP_CODE_URL_PREFIX.to_string(),
            };
            let client = AccountClient::new(cfg);
            Ok(Box::new(UserDemoApp::new(client)) as Box<dyn eframe::App>)
        }),
    )
}
