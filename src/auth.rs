use axum::http::HeaderMap;
use base64::Engine;
use subtle::ConstantTimeEq;

#[derive(Clone, Debug)]
pub struct Auth {
    token: Option<String>,
    basic_user: Option<String>,
    basic_password: Option<String>,
    pub required: bool,
}

impl Auth {
    pub fn new(
        token: Option<String>,
        basic_user: Option<String>,
        basic_password: Option<String>,
        required: bool,
    ) -> Self {
        Self {
            token: blank(token),
            basic_user: blank(basic_user),
            basic_password: blank(basic_password),
            required,
        }
    }

    pub fn allows(&self, headers: &HeaderMap) -> bool {
        if !self.required {
            return true;
        }
        let header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if let Some(token) = &self.token {
            if bearer_ok(header, token) {
                return true;
            }
        }
        if let (Some(user), Some(password)) = (&self.basic_user, &self.basic_password) {
            if basic_ok(header, user, password) {
                return true;
            }
        }
        false
    }

    pub fn basic_enabled(&self) -> bool {
        self.basic_user.is_some() && self.basic_password.is_some()
    }

    pub fn token_enabled(&self) -> bool {
        self.token.is_some()
    }
}

fn blank(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn bearer_ok(header: &str, expected: &str) -> bool {
    let Some(presented) = header.strip_prefix("Bearer ") else {
        return false;
    };
    ct_eq(presented.trim(), expected)
}

fn basic_ok(header: &str, user: &str, password: &str) -> bool {
    let Some(encoded) = header.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(decoded) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let Some((got_user, got_password)) = decoded.split_once(':') else {
        return false;
    };
    ct_eq(got_user, user) && ct_eq(got_password, password)
}

fn ct_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn bearer_and_basic_match_without_accepting_a_prefix() {
        let auth = Auth::new(
            Some("secret-token".to_string()),
            Some("ops".into()),
            Some("pw".into()),
            true,
        );
        let mut headers = HeaderMap::new();
        assert!(!auth.allows(&headers));
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token"),
        );
        assert!(auth.allows(&headers));
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token-extra"),
        );
        assert!(!auth.allows(&headers));
        let encoded = base64::engine::general_purpose::STANDARD.encode("ops:pw");
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {encoded}")).unwrap(),
        );
        assert!(auth.allows(&headers));
    }
}
