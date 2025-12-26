use axum::http::HeaderMap;
use axum::http::header::ACCEPT;
use mime::{Mime, Name};

pub const AP_ACCEPT: &str = r#"application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams""#;
pub const AP_RESPONSE_MIME: &str = r#"application/activity+json"#;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AcceptMimeSet: u8 {
        const HTML = 0b0001;
        const AP   = 0b0010;
        const JSON = 0b0100;
        const XML  = 0b1000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum AcceptMime {
    Xml = 0,
    Json = 1,
    AP = 2,
    Html = 3,
}

impl AcceptMime {
    fn to_singleton(self) -> AcceptMimeSet {
        match self {
            AcceptMime::Xml => AcceptMimeSet::XML,
            AcceptMime::Json => AcceptMimeSet::JSON,
            AcceptMime::AP => AcceptMimeSet::AP,
            AcceptMime::Html => AcceptMimeSet::HTML,
        }
    }
}

pub struct HeaderReader<'a> {
    accept: &'a str,
}

impl HeaderReader<'_> {
    pub fn new(headers: &HeaderMap) -> HeaderReader<'_> {
        let accept = headers.get(ACCEPT).and_then(|accept| accept.to_str().ok()).unwrap_or("");
        HeaderReader { accept }
    }

    pub fn select(&self, candidate: AcceptMimeSet) -> Option<AcceptMime> {
        mime::MimeIter::new(self.accept)
            .filter_map(Result::ok)
            .filter_map(|m| {
                let q_value = m
                    .get_param("q")
                    .map(|param| param.as_str())
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1.0);
                let m = if m.type_() == mime::STAR {
                    [AcceptMime::Html, AcceptMime::AP, AcceptMime::Json, AcceptMime::Xml]
                        .into_iter()
                        .find(|m| candidate.contains(m.to_singleton()))?
                } else if m.type_() == mime::TEXT {
                    if m.subtype() == mime::STAR {
                        [AcceptMime::Html, AcceptMime::Xml]
                            .into_iter()
                            .find(|m| candidate.contains(m.to_singleton()))?
                    } else if m.subtype() == mime::HTML {
                        AcceptMime::Html
                    } else if m.subtype() == mime::XML {
                        AcceptMime::Xml
                    } else {
                        return None;
                    }
                } else if m.type_() == mime::APPLICATION {
                    match (m.subtype().as_str(), m.suffix().as_ref().map(Name::as_str)) {
                        ("activity", Some("json")) => AcceptMime::AP,
                        ("ld", Some("json"))
                            if m.get_param("profile")
                                .is_some_and(|profile| profile == "https://www.w3.org/ns/activitystreams") =>
                        {
                            AcceptMime::AP
                        }
                        ("json", None) => AcceptMime::Json,
                        _ => return None,
                    }
                } else {
                    return None;
                };
                Some((q_value, m))
            })
            .filter(|(_, m)| candidate.contains(m.to_singleton()))
            .max_by(|(q1, mime1), (q2, mime2)| q1.total_cmp(q2).then_with(|| mime1.cmp(mime2)))
            .map(|(_, mime)| mime)
    }
}

pub fn is_content_type_ap(ty: &Mime) -> bool {
    if ty.type_() != mime::APPLICATION {
        return false;
    }
    match (ty.subtype().as_str(), ty.suffix().as_ref().map(Name::as_str)) {
        ("activity", Some("json")) => true,
        ("ld", Some("json"))
            if ty
                .get_param("profile")
                .is_some_and(|profile| profile == "https://www.w3.org/ns/activitystreams") =>
        {
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::headers::{AcceptMime, HeaderReader, is_content_type_ap};

    #[test]
    fn test_select() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "text/html;q=0.1, application/activity+json".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::HTML | AcceptMimeSet::AP;
        let selected = reader.select(candidate);
        assert_eq!(Some(AcceptMime::AP), selected);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "text/html".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::HTML | AcceptMimeSet::AP | AcceptMimeSet::JSON;
        let selected = reader.select(candidate);
        assert_eq!(Some(AcceptMime::Html), selected);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/json".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::AP | AcceptMimeSet::JSON;
        let selected = reader.select(candidate);
        assert_eq!(Some(AcceptMime::Json), selected);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/activity+json".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::HTML | AcceptMimeSet::JSON;
        let selected = reader.select(candidate);
        assert_eq!(None, selected);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/json, application/activity+json, text/html".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::AP | AcceptMimeSet::JSON | AcceptMimeSet::HTML;
        let selected = reader.select(candidate);
        assert_eq!(Some(AcceptMime::Html), selected);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, "application/json, application/activity+json".parse().unwrap());
        let reader = HeaderReader::new(&headers);
        let candidate = AcceptMimeSet::AP | AcceptMimeSet::JSON | AcceptMimeSet::HTML;
        let selected = reader.select(candidate);
        assert_eq!(Some(AcceptMime::AP), selected);
    }

    #[test]
    fn test_is_content_type_ap() {
        assert!(is_content_type_ap(&"application/activity+json".parse().unwrap()));
        assert!(is_content_type_ap(
            &r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#.parse().unwrap()
        ));
        assert!(!is_content_type_ap(&"application/ld+json".parse().unwrap()));
        assert!(!is_content_type_ap(&"application/json".parse().unwrap()));
        assert!(!is_content_type_ap(&"text/html".parse().unwrap()));
    }
}
