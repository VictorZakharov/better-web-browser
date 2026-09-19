//! Extra navigation state carried with a URL, never inferred from layout or a paint snapshot.
use serde::Deserialize;

pub const MAX_FORM_BODY_BYTES: usize = 128 * 1024;

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
    pub body: Vec<u8>,
}

impl NavigationOptions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.target.len() > 1024 || self.target.contains('\0') {
            return Err("invalid navigation target");
        }
        if let Some(post) = &self.post {
            if post.body.len() > MAX_FORM_BODY_BYTES {
                return Err("form submission exceeds the 128 KiB body limit");
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
