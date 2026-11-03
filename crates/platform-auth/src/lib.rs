use std::{
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl Role {
    pub fn can_create_purchase_orders(self) -> bool {
        matches!(self, Self::Admin | Self::Operator)
    }

    pub fn can_manage_replenishment(self) -> bool {
        self == Self::Admin
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Claims {
    pub sub: Uuid,
    pub tenant_id: Uuid,
    pub role: Role,
    pub iss: String,
    pub aud: String,
    pub iat: usize,
    pub exp: usize,
    pub jti: Uuid,
}

#[derive(Clone)]
pub struct JwtVerifier {
    decoding_key: Arc<DecodingKey>,
    issuer: Arc<str>,
    audience: Arc<str>,
}

#[derive(Clone)]
pub struct JwtSigner {
    encoding_key: Arc<EncodingKey>,
    issuer: Arc<str>,
    audience: Arc<str>,
}

impl JwtSigner {
    pub fn from_rsa_pem(
        path: &str,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let pem = fs::read(path)?;
        let encoding_key = EncodingKey::from_rsa_pem(&pem)?;
        Ok(Self {
            encoding_key: Arc::new(encoding_key),
            issuer: Arc::from(issuer.into()),
            audience: Arc::from(audience.into()),
        })
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        role: Role,
        lifetime: Duration,
    ) -> Result<(String, usize), TokenError> {
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| TokenError::Invalid)?
            .as_secs() as usize;
        let expires_at = issued_at.saturating_add(lifetime.as_secs() as usize);
        let claims = Claims {
            sub: user_id,
            tenant_id,
            role,
            iss: self.issuer.to_string(),
            aud: self.audience.to_string(),
            iat: issued_at,
            exp: expires_at,
            jti: Uuid::new_v4(),
        };

        encode(&Header::new(Algorithm::RS256), &claims, &self.encoding_key)
            .map(|token| (token, expires_at))
            .map_err(|_| TokenError::Signing)
    }
}

impl JwtVerifier {
    pub fn from_rsa_pem(
        path: &str,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let pem = fs::read(path)?;
        let decoding_key = DecodingKey::from_rsa_pem(&pem)?;
        Ok(Self {
            decoding_key: Arc::new(decoding_key),
            issuer: Arc::from(issuer.into()),
            audience: Arc::from(audience.into()),
        })
    }

    pub fn decode(&self, token: &str) -> Result<Claims, TokenError> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_ref()]);
        validation.set_audience(&[self.audience.as_ref()]);
        validation.set_required_spec_claims(&["aud", "exp", "iat", "iss", "sub"]);
        validation.leeway = 30;

        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| TokenError::Invalid)
    }
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("invalid bearer token")]
    Invalid,
    #[error("could not issue access token")]
    Signing,
}

pub async fn authenticate(
    State(verifier): State<JwtVerifier>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let Some(token) = token else {
        return unauthorized();
    };

    let Ok(claims) = verifier.decode(token) else {
        return unauthorized();
    };

    request.extensions_mut().insert(claims);
    next.run(request).await
}

pub async fn require_admin(request: Request, next: Next) -> Response {
    let is_admin = request
        .extensions()
        .get::<Claims>()
        .is_some_and(|claims| claims.role == Role::Admin);
    if !is_admin {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::Role;

    #[test]
    fn role_permissions_are_least_privilege() {
        assert!(Role::Admin.can_create_purchase_orders());
        assert!(Role::Operator.can_create_purchase_orders());
        assert!(!Role::Viewer.can_create_purchase_orders());
        assert!(Role::Admin.can_manage_replenishment());
        assert!(!Role::Operator.can_manage_replenishment());
        assert!(!Role::Viewer.can_manage_replenishment());
    }
}

fn unauthorized() -> Response {
    let mut response = (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}
