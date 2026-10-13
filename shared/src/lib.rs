wit_bindgen::generate!({ path: "wit", world: "runtime" });

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};

struct Module;

const HOOKS: [&str; 6] = [
    "api.pc.wallet.init",
    "api.mb.wallet.init",
    "api.pc.wallet.withdraw",
    "api.mb.wallet.withdraw",
    "api.pc.wallet.withdraw.before",
    "api.mb.wallet.withdraw.before",
];

fn own_slot() -> &'static str {
    if env!("CARGO_PKG_NAME") == "usd-wallet-pc" {
        "pc.global.body_end_html"
    } else {
        "mb.global.body_end_html"
    }
}

fn host_call(operation: &str, input: &Value) -> Result<Value, String> {
    let raw = lightsns::module::host::call(operation, &input.to_string())?;
    let envelope: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if envelope["ok"].as_bool() == Some(true) {
        Ok(envelope.get("data").cloned().unwrap_or(Value::Null))
    } else {
        Err(format!(
            "{}: {}",
            envelope["code"].as_str().unwrap_or("INTERNAL"),
            envelope["message"].as_str().unwrap_or("")
        ))
    }
}

fn register() -> Result<Value, String> {
    for hook in HOOKS {
        host_call("hook.on", &json!({"hook": hook}))?;
    }
    host_call("slot.on", &json!({"slot": own_slot()}))?;
    Ok(Value::Null)
}

fn unregister() -> Result<Value, String> {
    for hook in HOOKS {
        host_call("hook.off", &json!({"hook": hook}))?;
    }
    host_call("slot.off", &json!({"slot": own_slot()}))?;
    Ok(Value::Null)
}

fn filter_channel_array(value: &mut Value) {
    let Some(channels) = value.as_array_mut() else { return };
    channels.retain(|channel| channel["key"].as_str() == Some("bank"));
    for channel in channels {
        if let Some(obj) = channel.as_object_mut() {
            obj.insert("name".into(), json!("USDT"));
        }
    }
}

fn filter_withdraw(value: &mut Value) {
    let Some(withdraw) = value.as_object_mut() else { return };
    if let Some(channels) = withdraw.get_mut("channels") {
        filter_channel_array(channels);
    }
    if let Some(configs) = withdraw.get_mut("configs").and_then(Value::as_object_mut) {
        for config in configs.values_mut() {
            if let Some(channels) = config.get_mut("channels") {
                filter_channel_array(channels);
            }
        }
    }
    if let Some(list) = withdraw.get_mut("list").and_then(Value::as_array_mut) {
        for row in list {
            if row["channel"].as_str() == Some("bank")
                && matches!(row["account_extra"].as_str(), Some("TRC20" | "ERC20" | "Polygon"))
            {
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("channel_name".into(), json!("USDT"));
                }
            }
        }
    }
}

fn filter_response(input: &Value) -> Value {
    let mut body = input["value"].clone();
    if body["code"].as_i64() != Some(1) {
        return Value::Null;
    }
    if let Some(withdraw) = body.pointer_mut("/data/withdraw") {
        filter_withdraw(withdraw);
        body
    } else {
        Value::Null
    }
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err("请求正文编码不正确".into());
                }
                let hi = (bytes[i + 1] as char).to_digit(16).ok_or("请求正文编码不正确")?;
                let lo = (bytes[i + 2] as char).to_digit(16).ok_or("请求正文编码不正确")?;
                out.push(((hi << 4) | lo) as u8);
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).map_err(|_| "请求正文不是 UTF-8".into())
}

fn parse_body(input: &Value) -> Result<Map<String, Value>, String> {
    if input.pointer("/context/body_omitted").and_then(Value::as_bool) == Some(true) {
        return Err("提现请求正文过大或不可读取".into());
    }
    let body = input.pointer("/context/body").ok_or("缺少提现请求正文")?;
    if let Some(obj) = body.as_object() {
        return Ok(obj.clone());
    }
    let raw = body.as_str().ok_or("提现请求正文格式不正确")?;
    if raw.trim_start().starts_with('{') {
        return serde_json::from_str::<Map<String, Value>>(raw)
            .map_err(|_| "提现 JSON 正文格式不正确".into());
    }
    let mut parsed = Map::new();
    for part in raw.split('&') {
        if part.is_empty() { continue; }
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let key = percent_decode(key)?;
        let value = percent_decode(value)?;
        if parsed.insert(key, Value::String(value)).is_some() {
            return Err("提现请求包含重复参数".into());
        }
    }
    Ok(parsed)
}

fn field<'a>(body: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    body.get(name).and_then(Value::as_str).ok_or_else(|| format!("缺少 {name}"))
}

fn base58_value(c: u8) -> Option<u32> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    ALPHABET.iter().position(|x| *x == c).map(|n| n as u32)
}

fn validate_tron(address: &str) -> bool {
    if address.len() != 34 || !address.starts_with('T') { return false; }
    // Base58 digits are accumulated little endian in base 256.
    let mut digits: Vec<u8> = vec![0];
    for c in address.bytes() {
        let Some(mut carry) = base58_value(c) else { return false };
        for digit in &mut digits {
            carry += (*digit as u32) * 58;
            *digit = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            digits.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    digits.reverse();
    if digits.len() != 25 || digits[0] != 0x41 { return false; }
    let first = Sha256::digest(&digits[..21]);
    let second = Sha256::digest(first);
    digits[21..] == second[..4]
}

fn validate_evm(address: &str) -> bool {
    if address.len() != 42 || !address.starts_with("0x") { return false; }
    let hex = &address[2..];
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) { return false; }
    let lower = hex.to_ascii_lowercase();
    if hex == lower || hex == hex.to_ascii_uppercase() { return true; }
    let mut keccak = Keccak::v256();
    keccak.update(lower.as_bytes());
    let mut hash = [0u8; 32];
    keccak.finalize(&mut hash);
    for (i, c) in hex.bytes().enumerate() {
        if c.is_ascii_alphabetic() {
            let nibble = if i % 2 == 0 { hash[i / 2] >> 4 } else { hash[i / 2] & 0x0f };
            if c.is_ascii_uppercase() != (nibble >= 8) { return false; }
        }
    }
    true
}

fn check_withdraw(input: &Value) -> Result<(), String> {
    if input.pointer("/context/user_id").and_then(Value::as_u64).unwrap_or(0) == 0 {
        return Err("请先登录".into());
    }
    let body = parse_body(input)?;
    if field(&body, "channel")? != "bank" {
        return Err("仅支持 USDT 提现".into());
    }
    let account_name = field(&body, "account_name")?;
    if account_name.trim().is_empty() || account_name.chars().count() > 100 {
        return Err("收款账户标识无效".into());
    }
    let network = field(&body, "account_extra")?;
    let address = field(&body, "account_no")?;
    if address != address.trim() {
        return Err("钱包地址不能包含首尾空格".into());
    }
    let valid = match network {
        "TRC20" => validate_tron(address),
        "ERC20" | "Polygon" => validate_evm(address),
        _ => return Err("仅支持 TRC20、ERC20、Polygon 网络".into()),
    };
    if !valid {
        return Err(format!("{network} 钱包地址无效或校验和不正确"));
    }
    Ok(())
}

fn before_withdraw(input: &Value) -> Value {
    match check_withdraw(input) {
        Ok(()) => Value::Null,
        Err(message) => json!({"code": 403, "msg": message, "data": null}),
    }
}

fn dispatch(event: &str, input: &Value) -> Result<Value, String> {
    match event {
        "install" | "update" | "enable" => register(),
        "disable" | "uninstall" => unregister(),
        "slot.html" => {
            let slot = own_slot();
            if input["slots"].as_array().is_some_and(|slots| slots.iter().any(|v| v.as_str() == Some(slot))) {
                let mut output = Map::new();
                output.insert(slot.to_string(), json!("<span class=\"usd-wallet-mount\" hidden></span>"));
                Ok(Value::Object(output))
            } else {
                Ok(json!({}))
            }
        }
        "api.pc.wallet.init" | "api.mb.wallet.init"
        | "api.pc.wallet.withdraw" | "api.mb.wallet.withdraw" => Ok(filter_response(input)),
        "api.pc.wallet.withdraw.before" | "api.mb.wallet.withdraw.before" => Ok(before_withdraw(input)),
        other => Err(format!("模块事件不存在：{other}")),
    }
}

impl exports::lightsns::module::guest::Guest for Module {
    fn handle(event: String, input: String) -> Result<String, String> {
        let input: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
        dispatch(&event, &input).map(|value| value.to_string())
    }
}

export!(Module);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_validation() {
        assert!(validate_tron("T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb"));
        assert!(!validate_tron("T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwa"));
        assert!(validate_evm("0x52908400098527886E0F7030069857D2E4169EE7"));
        assert!(!validate_evm("0x52908400098527886E0F7030069857D2E4169Ee7"));
        assert!(validate_evm("0x000000000000000000000000000000000000dead"));
    }

    #[test]
    fn request_guard() {
        let good = json!({"context": {"user_id": 1, "body":
            "channel=bank&account_name=USDT&account_no=0x000000000000000000000000000000000000dead&account_extra=Polygon&amount=1"}});
        assert!(check_withdraw(&good).is_ok());
        for body in [
            "channel=alipay&account_name=USDT&account_no=0x000000000000000000000000000000000000dead&account_extra=Polygon",
            "channel=bank&account_name=USDT&account_no=abc&account_extra=Polygon",
            "channel=bank&account_name=USDT&account_no=0x000000000000000000000000000000000000dead&account_extra=BNB",
            "channel=bank&channel=bank&account_name=USDT&account_no=0x000000000000000000000000000000000000dead&account_extra=Polygon",
        ] {
            assert!(check_withdraw(&json!({"context": {"user_id": 1, "body": body}})).is_err());
        }
    }

    #[test]
    fn mobile_currency_channel_filter() {
        let input = json!({"value": {"code": 1, "data": {"withdraw": {
            "channels": [{"key":"bank","name":"银行卡"},{"key":"alipay","name":"支付宝"}],
            "configs": {"money": {"channels": [{"key":"wechat"},{"key":"bank","name":"银行卡"}]}}
        }}}});
        let filtered = filter_response(&input);
        assert_eq!(filtered["data"]["withdraw"]["channels"], json!([{"key":"bank","name":"USDT"}]));
        assert_eq!(filtered["data"]["withdraw"]["configs"]["money"]["channels"], json!([{"key":"bank","name":"USDT"}]));
    }
}
