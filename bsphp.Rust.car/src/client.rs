#![allow(dead_code)]

//! 账号模式（.lg）与卡模式（.ic）客户端。
use crate::crypto::{self, CryptoError};
use crate::encode::{encode_parameter, quote_parameter_payload};
use chrono::Local;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct AccountClientConfig {
    pub url: String,
    pub mutual_key: String,
    pub server_private_key: String,
    pub client_public_key: String,
    /// 图片验证码地址前缀，须带 sessl= 占位；完整地址为 前缀 + BSphpSeSsL。
    pub code_url_prefix: String,
}

#[derive(Debug, Clone)]
pub struct CardClientConfig {
    pub url: String,
    pub mutual_key: String,
    pub server_private_key: String,
    pub client_public_key: String,
}

#[derive(Debug, Clone)]
pub struct ApiResult {
    pub data: Option<Value>,
    pub code: Option<i64>,
}

impl ApiResult {
    pub fn message(&self) -> String {
        match &self.data {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("连接失败")]
    Connect,
    #[error("获取 BSphpSeSsL 失败")]
    Sessl,
}

pub const CODE_TYPE_LOGIN: &str = "INGES_LOGIN";
pub const CODE_TYPE_REG: &str = "INGES_RE";
pub const CODE_TYPE_BACK_PWD: &str = "INGES_MACK";
pub const CODE_TYPE_SAY: &str = "INGES_SAY";

pub const USER_INFO_FIELDS: &[(&str, &str)] = &[
    ("UserName", "用户名称"),
    ("UserUID", "用户UID"),
    ("UserReDate", "激活时间"),
    ("UserReIp", "激活时Ip"),
    ("UserIsLock", "用户状态"),
    ("UserLogInDate", "登录时间"),
    ("UserLogInIp", "登录Ip"),
    ("UserVipDate", "到期时"),
    ("UserKey", "绑定特征"),
    ("Class_Nane", "用户分组名称"),
    ("Class_Mark", "用户分组别名"),
    ("UserQQ", "用户QQ"),
    ("UserMAIL", "用户邮箱"),
    ("UserPayZhe", "购卡折扣"),
    ("UserTreasury", "是否代理"),
    ("UserMobile", "电话"),
    ("UserRMB", "帐号金额"),
    ("UserPoint", "帐号积分"),
    ("Usermibao_wenti", "密保问题"),
    ("UserVipWhether", "vip是否到期"),
    ("UserVipDateSurplus_DAY", "到期倒计时-天"),
    ("UserVipDateSurplus_H", "到期倒计时-时"),
    ("UserVipDateSurplus_I", "到期倒计时-分"),
    ("UserVipDateSurplus_S", "到期倒计时-秒"),
];

pub type UserInfoField = (&'static str, &'static str);

fn coerce_code(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn now_parts() -> (String, String) {
    let now = Local::now();
    let appsafecode = crypto::md5_hex(&now.format("%Y-%m-%d %H:%M:%S").to_string());
    let date_hash = now.format("%Y-%m-%d#%H:%M:%S").to_string();
    (appsafecode, date_hash)
}

/// 统一输出调试日志，便于定位请求加密/解密流程。
fn debug_http(label: &str, msg: impl AsRef<str>) {
    eprintln!("[BSPHP][{label}] {}", msg.as_ref());
}

/// 执行一次 AppEn 请求：
/// 参数拼装 -> AES 加密 -> RSA 包签名 -> HTTP POST -> 响应解密。
fn send_raw(
    http: &reqwest::blocking::Client,
    url: &str,
    mutual_key: &str,
    server_private_key: &str,
    client_public_key: &str,
    sessl: &str,
    api: &str,
    extra: &BTreeMap<String, String>,
) -> Result<Option<Map<String, Value>>, CryptoError> {
    let api_tag = format!("api={api}");
    let (appsafecode, date_hash) = now_parts();
    debug_http(
        "request.begin",
        format!("{api_tag} sessl_len={} url={url}", sessl.len()),
    );
    let mut param: BTreeMap<String, String> = BTreeMap::new();
    param.insert("api".into(), api.to_string());
    param.insert("BSphpSeSsL".into(), sessl.to_string());
    param.insert("date".into(), date_hash);
    param.insert("md5".into(), String::new());
    param.insert("mutualkey".into(), mutual_key.to_string());
    param.insert("appsafecode".into(), appsafecode.clone());
    for (k, v) in extra {
        param.insert(k.clone(), v.clone());
    }
    let mut pairs = Vec::new();
    for (k, v) in &param {
        pairs.push(format!("{}={}", k, encode_parameter(v)));
    }
    let data_str = pairs.join("&");
    debug_http("encrypt.before", format!("{api_tag} plain_query={data_str}"));
    let aes_key_full = crypto::md5_hex(&(server_private_key.to_string() + &appsafecode));
    let aes_key = &aes_key_full[..16.min(aes_key_full.len())];
    debug_http("encrypt.before", format!("{api_tag} aes_key16={aes_key}"));
    let enc_b64 = crypto::aes128_cbc_encrypt_base64(&data_str, aes_key)?;
    let sig_md5 = crypto::md5_hex(&enc_b64);
    let signature_content = format!("0|AES-128-CBC|{aes_key}|{sig_md5}|json");
    let rsa_b64 = crypto::rsa_encrypt_pkcs1_base64(&signature_content, client_public_key)?;
    let payload = format!("{enc_b64}|{rsa_b64}");
    let encoded = quote_parameter_payload(&payload);
    let body = format!("parameter={}", encoded);
    debug_http(
        "encrypt.after",
        format!(
            "{api_tag} enc_len={}, rsa_len={}, body_len={}",
            enc_b64.len(),
            rsa_b64.len(),
            body.len()
        ),
    );
    let resp = http
        .post(url)
        .header(
            "Content-Type",
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send();
    let Ok(r) = resp else {
        debug_http("request.result", format!("{api_tag} send_failed"));
        return Ok(None);
    };
    if r.status() != 200 {
        debug_http(
            "request.result",
            format!("{api_tag} non_200_status={}", r.status()),
        );
        return Ok(None);
    }
    let Ok(text) = r.text() else {
        debug_http("request.result", format!("{api_tag} response_read_failed"));
        return Ok(None);
    };
    let text = text.trim();
    if text.is_empty() {
        debug_http("request.result", format!("{api_tag} empty_response"));
        return Ok(None);
    }
    debug_http(
        "request.result",
        format!("{api_tag} raw_response_len={} raw={text}", text.len()),
    );
    let v = crypto::decrypt_response_body(text, server_private_key, &appsafecode)?;
    let code_dbg = v.get("code").map(|x| x.to_string()).unwrap_or_else(|| "null".to_string());
    let data_dbg = v.get("data").map(|x| x.to_string()).unwrap_or_else(|| "null".to_string());
    debug_http(
        "decrypt.result",
        format!("{api_tag} code={code_dbg} data={data_dbg} full_json={v}"),
    );
    Ok(v.as_object().cloned())
}

/// 统一把返回 map 转为前端展示结构（`code` + `data`）。
fn result_from_map(m: Option<Map<String, Value>>) -> ApiResult {
    let Some(m) = m else {
        return ApiResult {
            data: None,
            code: None,
        };
    };
    let code = m.get("code").and_then(coerce_code);
    let data = m.get("data").cloned();
    ApiResult { data, code }
}

// --- 账号模式 ---

pub struct AccountClient {
    http: reqwest::blocking::Client,
    cfg: AccountClientConfig,
    pub bs_php_sessl: String,
}

impl AccountClient {
    /// 创建账号模式客户端实例。
    pub fn new(cfg: AccountClientConfig) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("http client");
        Self {
            http,
            cfg,
            bs_php_sessl: String::new(),
        }
    }

    pub fn machine_code(&self) -> String {
        crate::machine::get_machine_code()
    }

    pub fn code_image_url(&self) -> String {
        if self.cfg.code_url_prefix.is_empty() {
            return String::new();
        }
        format!("{}{}", self.cfg.code_url_prefix, self.bs_php_sessl)
    }

    /// 发送账号模式 API 请求（自动走加密流程）。
    pub fn send(
        &mut self,
        api: &str,
        extra: &BTreeMap<String, String>,
    ) -> Result<Option<Map<String, Value>>, CryptoError> {
        send_raw(
            &self.http,
            &self.cfg.url,
            &self.cfg.mutual_key,
            &self.cfg.server_private_key,
            &self.cfg.client_public_key,
            &self.bs_php_sessl,
            api,
            extra,
        )
    }

    /// 测试网络是否可达（`internet.in`）。
    pub fn connect(&mut self) -> bool {
        let Ok(m) = self.send("internet.in", &BTreeMap::new()) else {
            return false;
        };
        m.as_ref()
            .and_then(|map| map.get("data"))
            .and_then(|d| d.as_str())
            .map(|s| s == "1")
            .unwrap_or(false)
    }

    /// 获取并刷新会话值 `BSphpSeSsL`。
    pub fn get_sessl(&mut self) -> bool {
        let Ok(m) = self.send("BSphpSeSsL.in", &BTreeMap::new()) else {
            return false;
        };
        let Some(map) = m else {
            return false;
        };
        if let Some(Value::String(s)) = map.get("data") {
            if !s.is_empty() {
                self.bs_php_sessl = s.clone();
                return true;
            }
        }
        false
    }

    /// 初始化：网络连通 + 获取会话。
    pub fn bootstrap(&mut self) -> Result<(), InitError> {
        if !self.connect() {
            return Err(InitError::Connect);
        }
        if !self.get_sessl() {
            return Err(InitError::Sessl);
        }
        Ok(())
    }

    fn result(&mut self, api: &str, extra: &BTreeMap<String, String>) -> ApiResult {
        result_from_map(self.send(api, extra).unwrap_or(None))
    }

    pub fn logout(&mut self) -> ApiResult {
        let r = self.result("cancellation.lg", &BTreeMap::new());
        self.bs_php_sessl.clear();
        let _ = self.get_sessl();
        r
    }

    pub fn get_notice(&mut self) -> ApiResult {
        self.result("gg.in", &BTreeMap::new())
    }

    pub fn get_version(&mut self) -> ApiResult {
        self.result("v.in", &BTreeMap::new())
    }

    pub fn get_soft_info(&mut self) -> ApiResult {
        self.result("miao.in", &BTreeMap::new())
    }

    pub fn get_server_date(&mut self) -> ApiResult {
        self.result("date.in", &BTreeMap::new())
    }

    pub fn get_preset_url(&mut self) -> ApiResult {
        self.result("url.in", &BTreeMap::new())
    }

    pub fn get_web_url(&mut self) -> ApiResult {
        self.result("weburl.in", &BTreeMap::new())
    }

    pub fn get_global_info(&mut self) -> ApiResult {
        self.result("globalinfo.in", &BTreeMap::new())
    }

    pub fn get_app_custom(&mut self, info: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("info".into(), info.to_string());
        self.result("appcustom.in", &m)
    }

    pub fn get_code_enabled_all(&mut self) -> ApiResult {
        self.result("getsetimag.in", &BTreeMap::new())
    }

    pub fn get_code_enabled_types(&mut self, types: &[&str]) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("type".into(), types.join("|"));
        self.result("getsetimag.in", &m)
    }

    pub fn get_code_enabled_single(&mut self, single: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("type".into(), single.to_string());
        self.result("getsetimag.in", &m)
    }

    pub fn get_logic_a(&mut self) -> ApiResult {
        self.result("logica.in", &BTreeMap::new())
    }

    pub fn get_logic_b(&mut self) -> ApiResult {
        self.result("logicb.in", &BTreeMap::new())
    }

    pub fn get_logic_info_a(&mut self) -> ApiResult {
        self.result("logicinfoa.in", &BTreeMap::new())
    }

    pub fn get_logic_info_b(&mut self) -> ApiResult {
        self.result("logicinfob.in", &BTreeMap::new())
    }

    pub fn get_end_time(&mut self) -> ApiResult {
        self.result("vipdate.lg", &BTreeMap::new())
    }

    pub fn get_user_info(&mut self, info: Option<&str>) -> ApiResult {
        let mut m = BTreeMap::new();
        if let Some(i) = info {
            m.insert("info".into(), i.to_string());
        }
        self.result("getuserinfo.lg", &m)
    }

    pub fn get_user_key(&mut self) -> ApiResult {
        self.result("userkey.lg", &BTreeMap::new())
    }

    pub fn heartbeat(&mut self) -> ApiResult {
        self.result("timeout.lg", &BTreeMap::new())
    }

    pub fn login(
        &mut self,
        user: &str,
        password: &str,
        code: &str,
        key: &str,
        maxoror: &str,
    ) -> ApiResult {
        let mc = if key.is_empty() {
            self.machine_code()
        } else {
            key.to_string()
        };
        let mx = if maxoror.is_empty() {
            mc.clone()
        } else {
            maxoror.to_string()
        };
        let mut ex = BTreeMap::new();
        ex.insert("user".into(), user.to_string());
        ex.insert("pwd".into(), password.to_string());
        ex.insert("coode".into(), code.to_string());
        ex.insert("key".into(), mc);
        ex.insert("maxoror".into(), mx);
        let Ok(m) = self.send("login.lg", &ex) else {
            return ApiResult {
                data: Some(Value::String("系统错误，登录失败！".into())),
                code: None,
            };
        };
        let Some(map) = m else {
            return ApiResult {
                data: Some(Value::String("系统错误，登录失败！".into())),
                code: None,
            };
        };
        let code = map.get("code").and_then(coerce_code);
        if matches!(code, Some(1011 | 9908)) {
            if let Some(Value::String(s)) = map.get("SeSsL") {
                self.bs_php_sessl = s.clone();
            }
        }
        let data = map.get("data").cloned();
        ApiResult { data, code }
    }

    pub fn send_email_code(&mut self, scene: &str, email: &str, coode: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("scene".into(), scene.to_string());
        m.insert("email".into(), email.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("send_email.lg", &m)
    }

    pub fn register_email(
        &mut self,
        user: &str,
        email: &str,
        email_code: &str,
        pwd: &str,
        pwdb: &str,
        key: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("email".into(), email.to_string());
        m.insert("email_code".into(), email_code.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("key".into(), key.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("register_email.lg", &m)
    }

    pub fn login_email(
        &mut self,
        email: &str,
        email_code: &str,
        key: &str,
        maxoror: &str,
        coode: &str,
    ) -> ApiResult {
        let mut ex = BTreeMap::new();
        ex.insert("email".into(), email.to_string());
        ex.insert("email_code".into(), email_code.to_string());
        ex.insert("key".into(), key.to_string());
        ex.insert("maxoror".into(), maxoror.to_string());
        ex.insert("coode".into(), coode.to_string());
        let Ok(m) = self.send("login_email.lg", &ex) else {
            return ApiResult {
                data: Some(Value::String("系统错误，邮箱验证码登录失败！".into())),
                code: None,
            };
        };
        result_from_map(m)
    }

    pub fn reset_email_pwd(
        &mut self,
        email: &str,
        email_code: &str,
        pwd: &str,
        pwdb: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("email".into(), email.to_string());
        m.insert("email_code".into(), email_code.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("resetpwd_email.lg", &m)
    }

    pub fn send_sms_code(&mut self, scene: &str, mobile: &str, area: &str, coode: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("scene".into(), scene.to_string());
        m.insert("mobile".into(), mobile.to_string());
        m.insert("area".into(), area.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("send_sms.lg", &m)
    }

    pub fn register_sms(
        &mut self,
        user: &str,
        mobile: &str,
        area: &str,
        sms_code: &str,
        pwd: &str,
        pwdb: &str,
        key: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("mobile".into(), mobile.to_string());
        m.insert("area".into(), area.to_string());
        m.insert("sms_code".into(), sms_code.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("key".into(), key.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("register_sms.lg", &m)
    }

    pub fn login_sms(
        &mut self,
        mobile: &str,
        area: &str,
        sms_code: &str,
        key: &str,
        maxoror: &str,
        coode: &str,
    ) -> ApiResult {
        let mut ex = BTreeMap::new();
        ex.insert("mobile".into(), mobile.to_string());
        ex.insert("area".into(), area.to_string());
        ex.insert("sms_code".into(), sms_code.to_string());
        ex.insert("key".into(), key.to_string());
        ex.insert("maxoror".into(), maxoror.to_string());
        ex.insert("coode".into(), coode.to_string());
        let Ok(m) = self.send("login_sms.lg", &ex) else {
            return ApiResult {
                data: Some(Value::String("系统错误，短信验证码登录失败！".into())),
                code: None,
            };
        };
        let Some(map) = m else {
            return ApiResult {
                data: Some(Value::String("系统错误，短信验证码登录失败！".into())),
                code: None,
            };
        };
        let code = map.get("code").and_then(coerce_code);
        if matches!(code, Some(1011 | 9908)) {
            if let Some(Value::String(s)) = map.get("SeSsL") {
                self.bs_php_sessl = s.clone();
            }
        }
        let data = map.get("data").cloned();
        ApiResult { data, code }
    }

    pub fn reset_sms_pwd(
        &mut self,
        mobile: &str,
        area: &str,
        sms_code: &str,
        pwd: &str,
        pwdb: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("mobile".into(), mobile.to_string());
        m.insert("area".into(), area.to_string());
        m.insert("sms_code".into(), sms_code.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("resetpwd_sms.lg", &m)
    }

    pub fn reg(
        &mut self,
        user: &str,
        pwd: &str,
        pwdb: &str,
        coode: &str,
        mobile: &str,
        mibao_wenti: &str,
        mibao_daan: &str,
        qq: &str,
        mail: &str,
        extension: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("coode".into(), coode.to_string());
        m.insert("mobile".into(), mobile.to_string());
        m.insert("mibao_wenti".into(), mibao_wenti.to_string());
        m.insert("mibao_daan".into(), mibao_daan.to_string());
        m.insert("qq".into(), qq.to_string());
        m.insert("mail".into(), mail.to_string());
        m.insert("extension".into(), extension.to_string());
        self.result("registration.lg", &m)
    }

    pub fn unbind(&mut self, user: &str, pwd: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("pwd".into(), pwd.to_string());
        self.result("jiekey.lg", &m)
    }

    pub fn pay(
        &mut self,
        user: &str,
        userpwd: &str,
        userset: bool,
        ka: &str,
        pwd: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("userpwd".into(), userpwd.to_string());
        m.insert(
            "userset".into(),
            if userset { "1".into() } else { "0".into() },
        );
        m.insert("ka".into(), ka.to_string());
        m.insert("pwd".into(), pwd.to_string());
        self.result("chong.lg", &m)
    }

    pub fn back_pass(
        &mut self,
        user: &str,
        pwd: &str,
        pwdb: &str,
        wenti: &str,
        daan: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("wenti".into(), wenti.to_string());
        m.insert("daan".into(), daan.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("backto.lg", &m)
    }

    pub fn edit_pass(
        &mut self,
        user: &str,
        pwd: &str,
        pwda: &str,
        pwdb: &str,
        img: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("pwda".into(), pwda.to_string());
        m.insert("pwdb".into(), pwdb.to_string());
        m.insert("img".into(), img.to_string());
        self.result("password.lg", &m)
    }

    pub fn feedback(
        &mut self,
        user: &str,
        pwd: &str,
        table: &str,
        qq: &str,
        leix: &str,
        text: &str,
        coode: &str,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("user".into(), user.to_string());
        m.insert("pwd".into(), pwd.to_string());
        m.insert("table".into(), table.to_string());
        m.insert("qq".into(), qq.to_string());
        m.insert("leix".into(), leix.to_string());
        m.insert("txt".into(), text.to_string());
        m.insert("coode".into(), coode.to_string());
        self.result("liuyan.in", &m)
    }
}

// --- 卡模式 ---

pub struct CardClient {
    http: reqwest::blocking::Client,
    cfg: CardClientConfig,
    pub bs_php_sessl: String,
}

impl CardClient {
    /// 创建卡模式客户端实例。
    pub fn new(cfg: CardClientConfig) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("http client");
        Self {
            http,
            cfg,
            bs_php_sessl: String::new(),
        }
    }

    pub fn machine_code(&self) -> String {
        crate::machine::get_machine_code()
    }

    /// 发送卡模式 API 请求（自动走加密流程）。
    pub fn send(
        &mut self,
        api: &str,
        extra: &BTreeMap<String, String>,
    ) -> Result<Option<Map<String, Value>>, CryptoError> {
        send_raw(
            &self.http,
            &self.cfg.url,
            &self.cfg.mutual_key,
            &self.cfg.server_private_key,
            &self.cfg.client_public_key,
            &self.bs_php_sessl,
            api,
            extra,
        )
    }

    /// 测试网络是否可达（`internet.in`）。
    pub fn connect(&mut self) -> bool {
        let Ok(m) = self.send("internet.in", &BTreeMap::new()) else {
            return false;
        };
        m.as_ref()
            .and_then(|map| map.get("data"))
            .and_then(|d| d.as_str())
            .map(|s| s == "1")
            .unwrap_or(false)
    }

    /// 获取并刷新会话值 `BSphpSeSsL`。
    pub fn get_sessl(&mut self) -> bool {
        let Ok(m) = self.send("BSphpSeSsL.in", &BTreeMap::new()) else {
            return false;
        };
        let Some(map) = m else {
            return false;
        };
        if let Some(Value::String(s)) = map.get("data") {
            if !s.is_empty() {
                self.bs_php_sessl = s.clone();
                return true;
            }
        }
        false
    }

    /// 初始化：网络连通 + 获取会话。
    pub fn bootstrap(&mut self) -> Result<(), InitError> {
        if !self.connect() {
            return Err(InitError::Connect);
        }
        if !self.get_sessl() {
            return Err(InitError::Sessl);
        }
        Ok(())
    }

    fn result(&mut self, api: &str, extra: &BTreeMap<String, String>) -> ApiResult {
        result_from_map(self.send(api, extra).unwrap_or(None))
    }

    pub fn logout(&mut self) -> ApiResult {
        let r = self.result("cancellation.ic", &BTreeMap::new());
        self.bs_php_sessl.clear();
        let _ = self.get_sessl();
        r
    }

    pub fn add_card_features(&mut self, carid: &str, key: &str, maxoror: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("carid".into(), carid.to_string());
        m.insert("key".into(), key.to_string());
        m.insert("maxoror".into(), maxoror.to_string());
        self.result("AddCardFeatures.key.ic", &m)
    }

    pub fn recharge_card(&mut self, icid: &str, ka: &str, pwd: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("icid".into(), icid.to_string());
        m.insert("ka".into(), ka.to_string());
        m.insert("pwd".into(), pwd.to_string());
        self.result("chong.ic", &m)
    }

    pub fn get_date_ic(&mut self) -> ApiResult {
        self.result("getdate.ic", &BTreeMap::new())
    }

    pub fn get_login_info(&mut self) -> ApiResult {
        self.result("getlkinfo.ic", &BTreeMap::new())
    }

    pub fn heartbeat(&mut self) -> ApiResult {
        self.result("timeout.ic", &BTreeMap::new())
    }

    pub fn get_notice(&mut self) -> ApiResult {
        self.result("gg.in", &BTreeMap::new())
    }

    pub fn get_version(&mut self) -> ApiResult {
        self.result("v.in", &BTreeMap::new())
    }

    pub fn get_soft_info(&mut self) -> ApiResult {
        self.result("miao.in", &BTreeMap::new())
    }

    pub fn get_server_date(&mut self) -> ApiResult {
        self.result("date.in", &BTreeMap::new())
    }

    pub fn get_preset_url(&mut self) -> ApiResult {
        self.result("url.in", &BTreeMap::new())
    }

    pub fn get_web_url(&mut self) -> ApiResult {
        self.result("weburl.in", &BTreeMap::new())
    }

    pub fn get_global_info(&mut self, info: Option<&str>) -> ApiResult {
        let mut m = BTreeMap::new();
        if let Some(i) = info {
            m.insert("info".into(), i.to_string());
        }
        self.result("globalinfo.in", &m)
    }

    pub fn get_app_custom(&mut self, info: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("info".into(), info.to_string());
        self.result("appcustom.in", &m)
    }

    pub fn get_logic_a(&mut self) -> ApiResult {
        self.result("logica.in", &BTreeMap::new())
    }

    pub fn get_logic_b(&mut self) -> ApiResult {
        self.result("logicb.in", &BTreeMap::new())
    }

    pub fn query_card(&mut self, cardid: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("cardid".into(), cardid.to_string());
        self.result("socard.in", &m)
    }

    pub fn get_card_info(
        &mut self,
        ic_carid: &str,
        ic_pwd: &str,
        info: &str,
        typ: Option<&str>,
    ) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("ic_carid".into(), ic_carid.to_string());
        m.insert("ic_pwd".into(), ic_pwd.to_string());
        m.insert("info".into(), info.to_string());
        if let Some(t) = typ {
            m.insert("type".into(), t.to_string());
        }
        self.result("getinfo.ic", &m)
    }

    pub fn bind_card(&mut self, key: &str, icid: &str, icpwd: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("key".into(), key.to_string());
        m.insert("icid".into(), icid.to_string());
        m.insert("icpwd".into(), icpwd.to_string());
        self.result("setcaron.ic", &m)
    }

    pub fn unbind_card(&mut self, icid: &str, icpwd: &str) -> ApiResult {
        let mut m = BTreeMap::new();
        m.insert("icid".into(), icid.to_string());
        m.insert("icpwd".into(), icpwd.to_string());
        self.result("setcarnot.ic", &m)
    }

    pub fn login_ic(
        &mut self,
        icid: &str,
        icpwd: &str,
        key: Option<&str>,
        maxoror: Option<&str>,
    ) -> ApiResult {
        let k = key
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.machine_code());
        let m_or = maxoror.map(|s| s.to_string()).unwrap_or_else(|| k.clone());
        let mut ex = BTreeMap::new();
        ex.insert("icid".into(), icid.to_string());
        ex.insert("icpwd".into(), icpwd.to_string());
        ex.insert("key".into(), k);
        ex.insert("maxoror".into(), m_or);
        let Ok(m) = self.send("login.ic", &ex) else {
            return ApiResult {
                data: Some(Value::String("系统错误，登录失败！".into())),
                code: None,
            };
        };
        let Some(map) = m else {
            return ApiResult {
                data: Some(Value::String("系统错误，登录失败！".into())),
                code: None,
            };
        };
        let code = map.get("code").and_then(coerce_code);
        if matches!(code, Some(1011 | 9908 | 1081)) {
            if let Some(Value::String(s)) = map.get("SeSsL") {
                self.bs_php_sessl = s.clone();
            }
        }
        let data = map.get("data").cloned();
        ApiResult { data, code }
    }
}

