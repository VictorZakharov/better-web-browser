//! Extra navigation state carried with a URL, never inferred from layout or a paint snapshot.
use base64::Engine as _;
use serde::{Deserialize, de::Error as _};

// A selected-file snapshot is limited to 4 MiB; leave room for multipart
// framing and ordinary fields while staying within the 8 MiB renderer frame.
pub const MAX_FORM_BODY_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_PENDING_FORM_BODY_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_FORM_NAVIGATION_JSON_BYTES: usize = 7 * 1024 * 1024;
const _: () = assert!(MAX_FORM_BODY_BYTES < crate::limits::MAX_FRAME_PAYLOAD);

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavigationOptions {
    pub target: String,
    pub post: Option<FormPost>,
    pub noreferrer: bool,
    pub replace_history: bool,
    #[serde(skip)]
    pub user_initiated: bool,
    #[serde(skip)]
    pub form_submission: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormPost {
    pub content_type: String,
    #[serde(deserialize_with = "deserialize_form_body")]
    pub body: Vec<u8>,
}

// The private script-host request uses base64 rather than a decimal byte array.
// Validate the encoded length before decoding so author-supplied JSON cannot
// allocate an unbounded binary body or expand a 5 MiB upload into millions of
// JSON array entries. Renderer presentation IPC still carries raw bytes.
fn deserialize_form_body<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let encoded = String::deserialize(deserializer)?;
    if encoded.len() > MAX_FORM_BODY_BYTES.div_ceil(3) * 4 {
        return Err(D::Error::custom("form body base64 exceeds the 5 MiB limit"));
    }
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(D::Error::custom)?;
    if body.len() > MAX_FORM_BODY_BYTES {
        return Err(D::Error::custom(
            "form submission exceeds the 5 MiB body limit",
        ));
    }
    Ok(body)
}

impl NavigationOptions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.target.len() > 1024 || self.target.contains('\0') {
            return Err("invalid navigation target");
        }
        if let Some(post) = &self.post {
            if post.body.len() > MAX_FORM_BODY_BYTES {
                return Err("form submission exceeds the 5 MiB body limit");
            }
            if post.content_type.len() > 256
                || post.content_type.contains(['\r', '\n', '\0'])
                || !(post.content_type == "application/x-www-form-urlencoded"
                    || post.content_type == "text/plain"
                    || post
                        .content_type
                        .starts_with("multipart/form-data; boundary="))
            {
                return Err("invalid form submission content type");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_upload_body_fits_one_renderer_frame() {
        let mut options = NavigationOptions {
            post: Some(FormPost {
                content_type: "multipart/form-data; boundary=boundary".into(),
                body: vec![0; MAX_FORM_BODY_BYTES],
            }),
            ..Default::default()
        };
        assert!(options.validate().is_ok());
        options.post.as_mut().unwrap().body.push(0);
        assert_eq!(
            options.validate(),
            Err("form submission exceeds the 5 MiB body limit")
        );
    }

    #[test]
    fn private_form_host_json_decodes_only_bounded_base64_bytes() {
        let options: NavigationOptions =
            serde_json::from_str(r#"{"post":{"content_type":"text/plain","body":"AP8="}}"#)
                .unwrap();
        assert_eq!(options.post.unwrap().body, [0, 255]);
        for body in [r#"[0,255]"#, r#""bad=""#] {
            let payload = format!(r#"{{"post":{{"content_type":"text/plain","body":{body}}}}}"#);
            assert!(serde_json::from_str::<NavigationOptions>(&payload).is_err());
        }
    }
}
