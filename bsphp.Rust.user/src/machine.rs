//! 本机设备标识（登录 key / maxoror），与演示工程持久化策略一致。

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

fn cache_path() -> PathBuf {
    let base = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bsphp_rust_demo");
    let _ = fs::create_dir_all(&base);
    base.join("machine_code.txt")
}

fn read_persisted() -> Option<String> {
    let p = cache_path();
    let mut s = String::new();
    fs::File::open(&p).ok()?.read_to_string(&mut s).ok()?;
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn write_persisted(s: &str) {
    if let Ok(mut f) = fs::File::create(cache_path()) {
        let _ = f.write_all(s.as_bytes());
    }
}

#[cfg(target_os = "macos")]
fn darwin_ioreg_uuid() -> Option<String> {
    let out = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(start) = text.find("\"IOPlatformUUID\"") {
        let sub = &text[start..];
        if let Some(eq) = sub.find('=') {
            let after = sub[eq + 1..].trim();
            if let Some(q1) = after.find('"') {
                let after = &after[q1 + 1..];
                if let Some(q2) = after.find('"') {
                    let u = after[..q2].trim();
                    if !u.is_empty() {
                        return Some(u.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn darwin_ioreg_uuid() -> Option<String> {
    None
}

/// 优先硬件 UUID（macOS），否则读缓存，否则生成并写入。
pub fn get_machine_code() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Some(u) = darwin_ioreg_uuid() {
            return u;
        }
    }
    if let Some(c) = read_persisted() {
        return c;
    }
    let s: String = uuid::Uuid::new_v4().as_simple().to_string();
    write_persisted(&s);
    s
}

