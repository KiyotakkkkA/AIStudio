//! Upload protocol as observed on the live client 2026-11-03 (FE 0.3.32).
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use crate::protocol::Observation;

pub const UPLOAD_PATH: &str = "/files/getstsToken";
/// Live capture: grant request and response, pre-signed `file_url`, and the client's own
/// OSS PUT to the `oss-accelerate` host with the scope region from the `region` field.
pub const UPLOAD_OBSERVATION: Observation = Observation {
    source: "POST /api/v2/files/getstsToken then PUT <file_url without query> (OSS4-HMAC-SHA256, FE 0.3.32)",
    observed_at: "2026-11-03",
};

/// The client sends the size as a string.
pub fn upload_body(name: &str, size: u64) -> Value {
    json!({"filename": name, "filesize": size.to_string(), "filetype": "image"})
}

/// Temporary object-store credentials; intentionally neither Debug nor Serialize.
pub struct UploadGrant {
    pub file_id: String,
    pub file_url: String,
    file_path: Option<String>,
    object_url: String,
    bucket: String,
    region: String,
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl UploadGrant {
    pub fn parse(payload: &Value) -> Option<Self> {
        if payload.get("success").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let data = payload.get("data").unwrap_or(payload);
        let field = |name: &str| {
            data.get(name)?
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let file_url = field("file_url")?;
        let mut url = Url::parse(&file_url).ok()?;
        let host = url.host_str()?;
        let (host_bucket, service) = host.split_once(".oss-")?;
        let host_region = service.strip_suffix(".aliyuncs.com")?;
        if url.scheme() != "https"
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || !valid_label(host_bucket)
            || !valid_label(host_region)
            || url.path() == "/"
        {
            return None;
        }
        // The bucket and the signing scope come from the grant fields: the host can be an
        // accelerate or CDN alias whose label is not the region (live: `oss-accelerate`
        // with `region: "oss-ap-southeast-1"`). The host labels are only a fallback.
        let bucket = field("bucketname").filter(|name| valid_label(name));
        let region = field("region").and_then(|value| {
            let stripped = value.strip_prefix("oss-").unwrap_or(&value);
            valid_label(stripped).then(|| stripped.to_owned())
        });
        let bucket = bucket.unwrap_or_else(|| host_bucket.to_owned());
        let region = region.unwrap_or_else(|| host_region.to_owned());
        url.set_query(None);
        url.set_fragment(None);
        Some(Self {
            file_id: field("file_id")?,
            file_url,
            file_path: field("file_path"),
            object_url: url.to_string(),
            bucket,
            region,
            access_key_id: field("access_key_id")?,
            access_key_secret: field("access_key_secret")?,
            security_token: field("security_token")?,
        })
    }

    pub fn object_url(&self) -> &str {
        &self.object_url
    }

    pub fn headers(&self, mime: &str, now_ms: i64) -> Vec<(String, String)> {
        let date = oss_date(now_ms);
        let mut headers = vec![
            ("content-type".to_owned(), mime.to_owned()),
            (
                "x-oss-content-sha256".to_owned(),
                "UNSIGNED-PAYLOAD".to_owned(),
            ),
            ("x-oss-date".to_owned(), date.clone()),
            (
                "x-oss-security-token".to_owned(),
                self.security_token.clone(),
            ),
            (
                "x-oss-user-agent".to_owned(),
                "aliyun-sdk-js/6.23.0 Chrome 156.0.0.0 on Windows 10 64-bit".to_owned(),
            ),
        ];
        let canonical_headers: String = headers
            .iter()
            .map(|(key, value)| format!("{key}:{value}\n"))
            .collect();
        let path = Url::parse(&self.object_url)
            .expect("validated object URL")
            .path()
            .to_owned();
        let canonical = format!(
            "PUT\n/{}{path}\n\n{canonical_headers}\n\nUNSIGNED-PAYLOAD",
            self.bucket
        );
        let day = &date[..8];
        let scope = format!("{day}/{}/oss/aliyun_v4_request", self.region);
        let hash: String = Sha256::digest(canonical.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let to_sign = format!("OSS4-HMAC-SHA256\n{date}\n{scope}\n{hash}");
        let date_key = hmac(
            format!("aliyun_v4{}", self.access_key_secret).as_bytes(),
            day.as_bytes(),
        );
        let region_key = hmac(&date_key, self.region.as_bytes());
        let service_key = hmac(&region_key, b"oss");
        let key = hmac(&service_key, b"aliyun_v4_request");
        let signature: String = hmac(&key, to_sign.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        headers.push((
            "authorization".to_owned(),
            format!(
                "OSS4-HMAC-SHA256 Credential={}/{scope},Signature={signature}",
                self.access_key_id
            ),
        ));
        headers
    }

    /// The client's `file` object carries `user_id`, the first segment of `file_path`.
    fn user_id(&self) -> Option<&str> {
        self.file_path
            .as_deref()?
            .split('/')
            .next()
            .filter(|segment| !segment.is_empty())
    }

    /// Mirrors the client's vision file entry; `itemId` and `uploadTaskId` differ there.
    pub fn attachment(
        &self,
        name: &str,
        mime: &str,
        size: u64,
        item_id: &str,
        task_id: &str,
        now_ms: i64,
    ) -> Value {
        let mut file = json!({"created_at": now_ms, "data": {}, "filename": name,
            "hash": null, "id": self.file_id,
            "meta": {"name": name, "size": size, "content_type": mime}, "update_at": now_ms});
        if let Some(user_id) = self.user_id() {
            file["user_id"] = json!(user_id);
        }
        json!({
            "type": "image", "file": file,
            "id": self.file_id, "url": self.file_url, "name": name, "collection_name": "", "progress": 0,
            "status": "uploaded", "greenNet": "success", "size": size, "error": "", "itemId": item_id,
            "file_type": mime, "showType": "image", "file_class": "vision", "uploadTaskId": task_id,
        })
    }
}

fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner = Sha256::new()
        .chain_update(block.map(|byte| byte ^ 0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(block.map(|byte| byte ^ 0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

fn oss_date(now_ms: i64) -> String {
    let seconds = now_ms.div_euclid(1000);
    let mut days = seconds.div_euclid(86400);
    let mut year = 1970;
    let leap = |year| year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for length in months {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    let time = seconds.rem_euclid(86400);
    format!(
        "{year:04}{month:02}{:02}T{:02}{:02}{:02}Z",
        days + 1,
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}

pub fn attach_images(mut body: Value, files: &[Value]) -> Value {
    if !files.is_empty() {
        body["messages"][0]["files"] = json!(files);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grant as captured 2026-11-03, with synthetic credentials.
    fn live_payload() -> Value {
        json!({"success":true,"data":{
            "file_id":"b726c853-8891-409f-a70f-b8c23dde5442",
            "file_url":"https://qwen-webui-prod.oss-accelerate.aliyuncs.com/d07d294d-fc9d-409c-b3b9-505c886acfbf/b726c853-8891-409f-a70f-b8c23dde5442_image.png?x-oss-signature-version=OSS4-HMAC-SHA256&x-oss-signature=abc",
            "file_path":"d07d294d-fc9d-409c-b3b9-505c886acfbf/b726c853-8891-409f-a70f-b8c23dde5442_image.png",
            "bucketname":"qwen-webui-prod",
            "region":"oss-ap-southeast-1",
            "endpoint":"oss-accelerate.aliyuncs.com",
            "access_key_id":"key", "access_key_secret":"secret", "security_token":"temporary"}})
    }

    #[test]
    fn the_accelerate_host_signs_with_the_grant_region_not_the_host_label() {
        let grant = UploadGrant::parse(&live_payload()).unwrap();
        assert_eq!(
            grant.object_url(),
            "https://qwen-webui-prod.oss-accelerate.aliyuncs.com/d07d294d-fc9d-409c-b3b9-505c886acfbf/b726c853-8891-409f-a70f-b8c23dde5442_image.png"
        );
        let headers = grant.headers("image/png", 1_700_000_000_000);
        assert!(
            headers
                .iter()
                .any(|(key, value)| key == "x-oss-date" && value == "20231114T221320Z")
        );
        assert!(headers.iter().any(|(key, value)| key == "authorization" && value.starts_with("OSS4-HMAC-SHA256 Credential=key/20231114/ap-southeast-1/oss/aliyun_v4_request,Signature=")));
    }

    #[test]
    fn the_attachment_mirrors_the_clients_vision_entry() {
        let grant = UploadGrant::parse(&live_payload()).unwrap();
        let entry = grant.attachment("image.png", "image/png", 100, "item", "task", 0);
        assert_eq!(entry["file_class"], "vision");
        assert_eq!(
            entry["file"]["user_id"],
            "d07d294d-fc9d-409c-b3b9-505c886acfbf"
        );
        assert_eq!(entry["itemId"], "item");
        assert_eq!(entry["uploadTaskId"], "task");
        assert!(
            entry["url"]
                .as_str()
                .unwrap()
                .contains("x-oss-signature=abc")
        );
        let body = attach_images(json!({"messages":[{"files":[]}]}), &[entry]);
        assert_eq!(body["messages"][0]["files"][0]["showType"], "image");
    }

    #[test]
    fn invalid_grants_are_refused() {
        let payload = live_payload();
        let mut invalid = payload.clone();
        invalid["success"] = json!(false);
        assert!(UploadGrant::parse(&invalid).is_none());
        invalid = payload.clone();
        invalid["data"]["file_url"] = json!("http://127.0.0.1/private");
        assert!(UploadGrant::parse(&invalid).is_none());
        invalid["data"]["file_url"] =
            json!("https://bucket.oss-ap-southeast-1.aliyuncs.com.evil.test/file");
        assert!(UploadGrant::parse(&invalid).is_none());
        invalid = payload.clone();
        invalid["data"]["region"] = json!("oss-../escape");
        assert!(UploadGrant::parse(&invalid).is_some()); // falls back to the host label
        invalid["data"]["file_url"] = json!("https://bucket.oss-.aliyuncs.com/file");
        assert!(UploadGrant::parse(&invalid).is_none());
    }

    #[test]
    fn the_grant_body_sends_the_size_as_a_string() {
        assert_eq!(
            upload_body("image.png", 1762),
            json!({"filename": "image.png", "filesize": "1762", "filetype": "image"})
        );
    }

    #[test]
    fn signing_primitive_matches_rfc_4231_and_dates_handle_leap_years() {
        let actual: String = hmac(b"Jefe", b"what do ya want for nothing?")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            actual,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(oss_date(1_709_164_800_000), "20240229T000000Z");
        let plain = json!({"messages":[{"files":[]}]});
        assert_eq!(attach_images(plain.clone(), &[]), plain);
    }
}
