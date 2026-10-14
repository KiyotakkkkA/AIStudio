use serde_json::json;
use zvs_contract::BrowserCookie;

#[derive(Debug, Clone, PartialEq)]
pub struct CookieMeta {
    pub domain: String,
    pub name: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub session: bool,
    pub expires: f64,
}

impl CookieMeta {
    pub fn id(&self) -> String {
        json!([self.domain, self.path, self.name, self.secure]).to_string()
    }

    pub fn dto(&self) -> BrowserCookie {
        BrowserCookie {
            id: self.id(),
            domain: self.domain.clone(),
            name: self.name.clone(),
            path: if self.path.is_empty() {
                "/".to_owned()
            } else {
                self.path.clone()
            },
            secure: self.secure,
            http_only: self.http_only,
            session: self.session,
            expiration_date: (!self.session && self.expires.is_finite() && self.expires >= 0.0)
                .then_some(self.expires),
        }
    }
}

pub fn cleared_domains(removed: &[CookieMeta]) -> Vec<String> {
    removed
        .iter()
        .filter(|cookie| !cookie.domain.is_empty())
        .map(|cookie| cookie.domain.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(domain: &str, session: bool, expires: f64) -> CookieMeta {
        CookieMeta {
            domain: domain.to_owned(),
            name: "sid".to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: true,
            session,
            expires,
        }
    }

    #[test]
    fn the_id_matches_the_electron_shape() {
        assert_eq!(
            cookie(".example.com", false, 0.0).id(),
            r#"[".example.com","/","sid",true]"#
        );
    }

    #[test]
    fn the_dto_carries_metadata_and_no_value() {
        let dto = cookie(".example.com", false, 1_800_000_000.5).dto();
        assert_eq!(
            dto,
            BrowserCookie {
                id: r#"[".example.com","/","sid",true]"#.to_owned(),
                domain: ".example.com".to_owned(),
                name: "sid".to_owned(),
                path: "/".to_owned(),
                secure: true,
                http_only: true,
                session: false,
                expiration_date: Some(1_800_000_000.5),
            }
        );
        let value = serde_json::to_value(&dto).expect("serialises");
        assert!(value.get("value").is_none());
    }

    #[test]
    fn session_cookies_have_no_expiry() {
        assert_eq!(cookie("a.example", true, -1.0).dto().expiration_date, None);
        assert_eq!(cookie("a.example", false, -1.0).dto().expiration_date, None);
    }

    #[test]
    fn an_empty_path_reads_as_root() {
        let mut meta = cookie("a.example", true, -1.0);
        meta.path.clear();
        assert_eq!(meta.dto().path, "/");
    }

    #[test]
    fn every_removed_cookie_with_a_domain_is_reported() {
        let removed = [
            cookie(".one.example", true, -1.0),
            cookie("", true, -1.0),
            cookie("two.example", true, -1.0),
        ];
        assert_eq!(cleared_domains(&removed), [".one.example", "two.example"]);
    }
}
