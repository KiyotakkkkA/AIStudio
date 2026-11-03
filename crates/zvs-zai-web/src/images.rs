//! Reference-derived upload protocol, reviewed 2026-11-03; pending live-client verification.
use serde_json::{Value, json};

pub const UPLOAD_PATH: &str = "/v1/files/";
pub const UPLOAD_REFERENCE_DATE: &str = "2026-11-03";
pub const UPLOAD_LIVE_VERIFIED: bool = false;

pub fn file_entry(
    file: &Value,
    name: &str,
    size: u64,
    item_id: &str,
    message_id: &str,
    now_ms: i64,
) -> Option<Value> {
    let id = file
        .get("id")?
        .as_str()
        .filter(|id| !id.is_empty() && !id.contains(['/', '\\', '?', '#']))?;
    Some(
        json!({"type":"image", "file":file, "id":id, "url":format!("/api/v1/files/{id}"),
        "name":name, "status":"uploaded", "size":size, "error":"", "itemId":item_id,
        "media":"image", "uploadedAt":now_ms, "ref_user_msg_id":message_id}),
    )
}

/// Preserve each image's owning message, and keep the text-only signature prompt unchanged.
pub fn attach_images(mut body: Value, images: &[(usize, Value)]) -> Option<Value> {
    if images.is_empty() {
        return Some(body);
    }
    for (index, file) in images {
        let message = body.get_mut("messages")?.get_mut(*index)?;
        let content = message.get_mut("content")?;
        if let Some(text) = content.as_str() {
            *content = json!([{"type":"text", "text":text}]);
        }
        content
            .as_array_mut()?
            .push(json!({"type":"image_url", "image_url":{"url":file.get("id")?.as_str()?}}));
    }
    body["files"] = images.iter().map(|(_, file)| file.clone()).collect();
    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attaches_file_ids_to_their_own_messages_and_keeps_signing_text() {
        let file = file_entry(
            &json!({"id":"file-1","filename":"image.png"}),
            "image.png",
            50,
            "item",
            "user",
            123,
        )
        .unwrap();
        let plain = json!({"messages":[{"role":"user","content":"describe"},{"role":"assistant","content":"answer"},{"role":"user","content":"followup"}], "signature_prompt":"followup"});
        assert_eq!(attach_images(plain.clone(), &[]), Some(plain.clone()));
        let body = attach_images(plain, &[(0, file)]).unwrap();
        assert_eq!(
            body["messages"][0]["content"][1]["image_url"]["url"],
            "file-1"
        );
        assert_eq!(body["messages"][2]["content"], "followup");
        assert_eq!(body["signature_prompt"], "followup");
        assert_eq!(body["files"][0]["ref_user_msg_id"], "user");
        assert!(file_entry(&json!({"id":"../escape"}), "x", 1, "item", "user", 0).is_none());
    }
}
