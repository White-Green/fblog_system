use axum::body::Body;
use axum::extract::Request;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;
use worker::Env;
use worker::send::SendFuture;

const ACCESS_JWT_HEADER: &str = "cf-access-jwt-assertion";
const ACCESS_EMAIL_HEADER: &str = "cf-access-authenticated-user-email";
const ADMIN_EMAIL_VAR: &str = "ADMIN_EMAIL";
const TEAM_DOMAIN_VAR: &str = "CLOUDFLARE_ACCESS_TEAM_DOMAIN";
const POLICY_AUD_VAR: &str = "CLOUDFLARE_ACCESS_AUD";

#[derive(Debug)]
pub enum AdminAuthError {
    MissingConfig(&'static str),
    MissingJwt,
    InvalidJwt(&'static str),
    InvalidEmailHeader,
    UnauthorizedEmail,
    FetchCerts(worker::Error),
    CertsStatus(u16),
    Decode(base64::DecodeError),
    Json(serde_json::Error),
    Signature,
}

impl AdminAuthError {
    pub fn log_message(&self) -> String {
        match self {
            Self::MissingConfig(name) => format!("missing config: {name}"),
            Self::MissingJwt => "missing Cloudflare Access JWT".to_string(),
            Self::InvalidJwt(reason) => format!("invalid Cloudflare Access JWT: {reason}"),
            Self::InvalidEmailHeader => "invalid Cloudflare Access email header".to_string(),
            Self::UnauthorizedEmail => "unauthorized admin email".to_string(),
            Self::FetchCerts(error) => format!("failed to fetch Cloudflare Access certs: {error}"),
            Self::CertsStatus(status) => format!("Cloudflare Access certs returned HTTP {status}"),
            Self::Decode(error) => format!("failed to decode Cloudflare Access JWT: {error}"),
            Self::Json(error) => format!("failed to parse Cloudflare Access JWT JSON: {error}"),
            Self::Signature => "invalid Cloudflare Access JWT signature".to_string(),
        }
    }
}

#[derive(Debug)]
struct AdminAuthConfig {
    admin_email: String,
    team_domain: String,
    policy_aud: String,
}

pub struct AdminAuthRequest {
    config: AdminAuthConfig,
    header_email: Option<String>,
    token: String,
}

#[derive(Debug, Deserialize)]
struct JwtHeader {
    alg: String,
    kid: String,
}

#[derive(Debug, Deserialize)]
struct AccessClaims {
    aud: Audience,
    email: Option<String>,
    exp: i64,
    iss: String,
    nbf: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct AccessCerts {
    keys: Vec<AccessJwk>,
}

#[derive(Debug, Deserialize)]
struct AccessJwk {
    alg: Option<String>,
    e: String,
    kid: String,
    kty: String,
    n: String,
    #[serde(rename = "use")]
    key_use: Option<String>,
}

impl AdminAuthConfig {
    fn from_env(env: &Env) -> Result<Self, AdminAuthError> {
        Ok(Self {
            admin_email: required_var(env, ADMIN_EMAIL_VAR)?,
            team_domain: normalize_team_domain(required_var(env, TEAM_DOMAIN_VAR)?),
            policy_aud: required_var(env, POLICY_AUD_VAR)?,
        })
    }
}

impl Audience {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::One(aud) => aud == expected,
            Self::Many(audiences) => audiences.iter().any(|aud| aud == expected),
        }
    }
}

pub fn read_admin_auth_request(req: &Request, env: &Env) -> Result<AdminAuthRequest, AdminAuthError> {
    let config = AdminAuthConfig::from_env(env)?;
    let token = req
        .headers()
        .get(ACCESS_JWT_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or(AdminAuthError::MissingJwt)?;
    let header_email = req
        .headers()
        .get(ACCESS_EMAIL_HEADER)
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()
        .map_err(|_| AdminAuthError::InvalidEmailHeader)?;

    Ok(AdminAuthRequest { config, header_email, token })
}

pub async fn authenticate_admin_request(request: AdminAuthRequest) -> Result<(), AdminAuthError> {
    let claims = validate_access_jwt(&request.token, &request.config).await?;
    let email = claims.email.as_deref().ok_or(AdminAuthError::InvalidJwt("missing email claim"))?;
    if !emails_match(email, &request.config.admin_email) {
        return Err(AdminAuthError::UnauthorizedEmail);
    }

    if let Some(header_email) = request.header_email
        && !emails_match(&header_email, email)
    {
        return Err(AdminAuthError::InvalidEmailHeader);
    }

    Ok(())
}

pub fn forbidden_response() -> http::Response<Body> {
    http::Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("Forbidden"))
        .unwrap()
}

async fn validate_access_jwt(token: &str, config: &AdminAuthConfig) -> Result<AccessClaims, AdminAuthError> {
    let ParsedJwt {
        header,
        claims,
        signing_input,
        signature,
    } = parse_jwt(token)?;

    if header.alg != "RS256" {
        return Err(AdminAuthError::InvalidJwt("unsupported alg"));
    }

    verify_claims(&claims, config)?;
    let certs = fetch_access_certs(&config.team_domain).await?;
    let key = certs
        .keys
        .iter()
        .find(|key| key.kid == header.kid)
        .ok_or(AdminAuthError::InvalidJwt("unknown kid"))?;
    verify_key_metadata(key)?;
    verify_signature(key, signing_input.as_bytes(), &signature)?;

    Ok(claims)
}

struct ParsedJwt {
    header: JwtHeader,
    claims: AccessClaims,
    signing_input: String,
    signature: Vec<u8>,
}

fn parse_jwt(token: &str) -> Result<ParsedJwt, AdminAuthError> {
    let mut parts = token.split('.');
    let header_part = parts.next().ok_or(AdminAuthError::InvalidJwt("missing header"))?;
    let claims_part = parts.next().ok_or(AdminAuthError::InvalidJwt("missing claims"))?;
    let signature_part = parts.next().ok_or(AdminAuthError::InvalidJwt("missing signature"))?;
    if parts.next().is_some() {
        return Err(AdminAuthError::InvalidJwt("too many jwt parts"));
    }

    let header = decode_json(header_part)?;
    let claims = decode_json(claims_part)?;
    let signature = URL_SAFE_NO_PAD.decode(signature_part).map_err(AdminAuthError::Decode)?;

    Ok(ParsedJwt {
        header,
        claims,
        signing_input: format!("{header_part}.{claims_part}"),
        signature,
    })
}

fn decode_json<T: for<'de> Deserialize<'de>>(encoded: &str) -> Result<T, AdminAuthError> {
    let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(AdminAuthError::Decode)?;
    serde_json::from_slice(&decoded).map_err(AdminAuthError::Json)
}

async fn fetch_access_certs(team_domain: &str) -> Result<AccessCerts, AdminAuthError> {
    let url = format!("{team_domain}/cdn-cgi/access/certs");
    let url = url.parse().map_err(|error| AdminAuthError::FetchCerts(worker::Error::from(error)))?;
    SendFuture::new(async move {
        let mut response = worker::Fetch::Url(url).send().await.map_err(AdminAuthError::FetchCerts)?;
        if !(200..300).contains(&response.status_code()) {
            return Err(AdminAuthError::CertsStatus(response.status_code()));
        }
        response.json::<AccessCerts>().await.map_err(AdminAuthError::FetchCerts)
    })
    .await
}

fn verify_claims(claims: &AccessClaims, config: &AdminAuthConfig) -> Result<(), AdminAuthError> {
    if normalize_team_domain(claims.iss.clone()) != config.team_domain {
        return Err(AdminAuthError::InvalidJwt("issuer mismatch"));
    }
    if !claims.aud.contains(&config.policy_aud) {
        return Err(AdminAuthError::InvalidJwt("audience mismatch"));
    }

    let now = Utc::now().timestamp();
    if claims.exp <= now {
        return Err(AdminAuthError::InvalidJwt("expired"));
    }
    if claims.nbf.is_some_and(|nbf| nbf > now) {
        return Err(AdminAuthError::InvalidJwt("not yet valid"));
    }

    Ok(())
}

fn verify_key_metadata(key: &AccessJwk) -> Result<(), AdminAuthError> {
    if key.kty != "RSA" {
        return Err(AdminAuthError::InvalidJwt("unsupported key type"));
    }
    if key.alg.as_deref().is_some_and(|alg| alg != "RS256") {
        return Err(AdminAuthError::InvalidJwt("unsupported key alg"));
    }
    if key.key_use.as_deref().is_some_and(|key_use| key_use != "sig") {
        return Err(AdminAuthError::InvalidJwt("unsupported key use"));
    }
    Ok(())
}

fn verify_signature(key: &AccessJwk, signing_input: &[u8], signature: &[u8]) -> Result<(), AdminAuthError> {
    let n = URL_SAFE_NO_PAD.decode(&key.n).map_err(AdminAuthError::Decode)?;
    let e = URL_SAFE_NO_PAD.decode(&key.e).map_err(AdminAuthError::Decode)?;
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signing_input, signature)
        .map_err(|_| AdminAuthError::Signature)
}

fn required_var(env: &Env, name: &'static str) -> Result<String, AdminAuthError> {
    env.var(name)
        .map(|value| value.to_string())
        .map_err(|_| AdminAuthError::MissingConfig(name))
}

fn normalize_team_domain(value: String) -> String {
    value.trim_end_matches('/').to_string()
}

fn emails_match(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_access_jwt_parts() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","kid":"key-1"}"#);
        let claims =
            URL_SAFE_NO_PAD.encode(r#"{"aud":["aud-1"],"email":"admin@example.com","exp":4102444800,"iss":"https://team.example.com","nbf":0}"#);
        let signature = URL_SAFE_NO_PAD.encode([1, 2, 3]);

        let parsed = parse_jwt(&format!("{header}.{claims}.{signature}")).unwrap();

        assert_eq!(parsed.header.alg, "RS256");
        assert_eq!(parsed.header.kid, "key-1");
        assert!(parsed.claims.aud.contains("aud-1"));
        assert_eq!(parsed.claims.email.as_deref(), Some("admin@example.com"));
        assert_eq!(parsed.signature, [1, 2, 3]);
    }

    #[test]
    fn verifies_claims_and_email_matching() {
        let config = AdminAuthConfig {
            admin_email: " Admin@Example.com ".to_string(),
            team_domain: "https://team.example.com".to_string(),
            policy_aud: "aud-1".to_string(),
        };
        let claims = AccessClaims {
            aud: Audience::Many(vec!["other-aud".to_string(), "aud-1".to_string()]),
            email: Some("admin@example.com".to_string()),
            exp: 4_102_444_800,
            iss: "https://team.example.com/".to_string(),
            nbf: Some(0),
        };

        assert!(verify_claims(&claims, &config).is_ok());
        assert!(emails_match(" admin@example.com ", &config.admin_email));
    }

    #[test]
    fn rejects_invalid_access_jwt_shape() {
        assert!(matches!(parse_jwt("not-a-jwt"), Err(AdminAuthError::InvalidJwt("missing claims"))));
        assert!(matches!(parse_jwt("a.b.c.d"), Err(AdminAuthError::InvalidJwt("too many jwt parts"))));
    }
}
