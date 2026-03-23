//! 账号模式演示 — 集中配置（须与 BSPHP 后台「当前应用」一致）。
//!
//! 【字段说明】
//! - `BSPHP_URL`：AppEn 完整入口（含 appid、m 通信密码、lang 等），POST 加密包发往该地址。
//! - `BSPHP_MUTUAL_KEY`：通信密钥 mutualkey，与后台应用绑定。
//! - `BSPHP_SERVER_PRIVATE_KEY`：服务器私钥 Base64（PKCS#8 DER），用于解密服务端返回的 RSA 段并得到 AES 密钥。
//! - `BSPHP_CLIENT_PUBLIC_KEY`：客户端公钥 Base64（SubjectPublicKeyInfo DER），用于加密请求签名段。
//! - `BSPHP_CODE_URL_PREFIX`：图片验证码地址前缀，须带 sessl= 占位；完整地址为 前缀 + BSphpSeSsL。
//! - `BSPHP_WEB_LOGIN_URL`：网页登录页基址，末尾 BSphpSeSsL= 由程序拼接当前会话令牌。
//! - `BSPHP_RENEW_*`：续费/购卡 Web 页（非 AppEn），daihao 须与后台软件代号一致。
//!
//! 【与本仓库「卡模式」演示工程 `bsphp.Rust.car` 的差异】
//! - 账号模式使用 .lg 等业务接口与 8888888 演示应用；卡模式使用 .ic 接口与另一套 appid/密钥。
//! - 卡模式工程无图片验证码前缀配置：主流程不展示图片验证码输入区。
//! - 网页登录、控制台「续费订阅」链接中的 daihao 在两工程中可能不同，须分别与后台销售配置一致。
//!
//! 【界面主题】本工程采用「生态 A」品牌绿 + 深灰辅色（与 `bsphp.Rust.car` 的「生态 B」紫青辅色区分，便于并排对比）。

//初始化地址
pub const BSPHP_URL: &str = "https://demo.bsphp.com/AppEn.php?appid=8888888&m=95e87faf2f6e41babddaef60273489e1&lang=0";
//通信密钥
pub const BSPHP_MUTUAL_KEY: &str = "6600cfcd5ac01b9bb3f2460eb416daa8";
//服务器私钥
pub const BSPHP_SERVER_PRIVATE_KEY: &str = "MIIEqAIBADANBgkqhkiG9w0BAQEFAASCBJIwggSOAgEAAoH+DEr7H5BhMwRA9ZWXVftcCWHznBdl0gQBu5617qSe9in+uloF1sC64Ybdc8Q0JwQkGQANC5PnMqPLgXXIfnl7/LpnQ/BvghQI5cr/4DEezRKrmQaXgYfXHL3woVw7JIsLpPTGa7Ar9S6SEH8RcPIbZjlPVRZPwV3RgWgox2/4lkXsmopqD+mEtOI/ntvti147nEpK2c7cdtCU5M2hQSlIXsTWvri88RTYJ/CtopBOXarUkNBfpWGImiYGsmbZI+YZ6uU0wSYlq8huu+pkTseUUiymzmv8Rpg3coi7YU+pszvB9wnQ1Rz6Z/B6Z3WN7d6OP7f9w0Q0WvgrsKcEJhMCAwEAAQKB/gHa5t6yiRiL0cm902K0VgVMdNjfZww0cpZ/svDaguqfF8PDhhIMb6dNFOo9d6lTpKbpLQ7MOR2ZPkLBJYqAhsdy0dac2BcHMviKk+afQwirgp3LMt3nQ/0gZMnVA0/Wc+Fm1vK1WUzcxEodAuLKhnv8tg4fGdYSdGVU9KJ0MU1bKQZXv0CAIhJYWsiCa5y5bFO7K+ia+UIVBHcvITQLzlgEm+Z/X6ye5cws4pWbk8+spsBDvweb5jpelbkCYs5C5TRNIWXk7+QxTXTg1vrcsmZRcmpRJq7sOd3faZltNHTIlB3HhWnsf47Bz334j9RtU8iqonbuBmcnYbD3+bvBAn891RGdAl+rVU/sJ2kPXmV4eqJOwJfbi8o1WYDp4GcK0ThjrZ1pmaZMj2WTjb3QX1VUoi+7l3389KzzDn0VXLKXZvGxmLikA1FWuuLUmwfNTxyxtGTBVeZCEaQ2lEJuaDGsK0oLi4Bo8ELfQw6JFK7jlgtTlflcYcul99P9BThDAn8y5TpSQy8/07LCgMMZOgJomYzQUmd14Zn2VQLH1u1Z4v2CPlOzGanDt7mmGZCew7iMSO1P0TrwDIreKzYyERuVvZti/IFHH1+J1hAbvk9SJGmdt46W5lyIp3xjdR2QmiK+hSsc8HF9R+zPaSe9yGA8+FwxLRfo0snGP3MC3aXxAn4n2iyABgejZlkc3EnanfzIqkHygC9gUbkCqa1tEDVZw3+Uv1G1vlJxBftyHuk4ZDmbUu1w+zM41nqiLbRxEE4LR06AKO7Yx0qlm86XOVTN/y9/WcWW1saRzs0IYIZwordhQIV463DYMgLn41B7Cdmu1gZ22TLfWCjpz9HSQosCfwMJu9l9OSzOLjV+CidPVyV3RPiKcrKOrOoPWQMkyTY8XnWP0t82APQ121cW35Mai8GT+NZy3tnFZeStH6cNbmAZ2VSnTfA45zMLHBsL2SBGHCfV9ST8yzk9BifJreIb0UceG9y2XY/k4zXeSQkDFPuOt7IXxv2W14SF9Q+Ou4ECfzfRP1hXPwq2w4YJ8sLmqWJT+3aMDucei5MJEAJNifZWhdW0GIrlKRSbhIgLAunxq+KK+mAPqqWw7Prsa21JbXSe3gugusu5d6ESURvLENRKI+Pp9TgRESsydeLy8VcPKRJ5/Ct7/p6QB3A+7F/iPNE2GagGffG9i7e+OdcToYQ=";
//客户端公
pub const BSPHP_CLIENT_PUBLIC_KEY: &str = "MIIBHjANBgkqhkiG9w0BAQEFAAOCAQsAMIIBBgKB/g26m2hYtESqcKW+95Lr+PfCd4bwHW2Z+mM0/vcKQ5j/ZGMigqkgl3QXCEcsCaw0KFSmqAPtLbrl6p5Sp+ZUSYEYQhSxAajE5qRCd3k0r/MIQQanBaOALkP71/u6U2SZhrTXd05n1wQo6ojMH/xVunBOFOa/Eon/Y5FVh6GiJpwwDkFzTlnecmff7Y+VDqRhZ7vu2CQjApOx23N6DiFEmVZYEb/efyASngoZ+3A/DSB5cwbaYVZ21EhPe/GNcwtUleFHn+d4vb0cvolO3Gyw6ObceOT/Q7E3k8ejIml6vPKzmRdtw0FXGOJTclx1CjShRDfXoUjFGyXHy3sZs9VLAgMBAAE=";
//图片验证码地址前缀
pub const BSPHP_CODE_URL_PREFIX: &str = "https://demo.bsphp.com/index.php?m=coode&sessl=";
//网页登录页基址
pub const BSPHP_WEB_LOGIN_URL: &str =
    "https://demo.bsphp.com/index.php?m=webapi&c=software_auth&a=index&daihao=8888888&BSphpSeSsL=";
//续费/购卡 Web 页（非 AppEn）
pub const BSPHP_RENEW_URL: &str =
    "https://demo.bsphp.com/index.php?m=webapi&c=salecard_renew&a=index&daihao=8888888";
pub const BSPHP_RENEW_CARD_URL: &str =
    "https://demo.bsphp.com/index.php?m=webapi&c=salecard_gencard&a=index&daihao=8888888";
pub const BSPHP_RENEW_STOCK_CARD_URL: &str =
    "https://demo.bsphp.com/index.php?m=webapi&c=salecard_salecard&a=index&daihao=8888888";
