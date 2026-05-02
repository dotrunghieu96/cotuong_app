// Auth core: password hashing, token generation, input validation.
//
// Higher-level pieces live in sibling modules:
//   - `session`: cookie + Axum extractor for the current user
//   - `handlers`: signup / login / logout / me HTTP routes
//   - `oauth` (feature `oauth`): Google OAuth flow
//   - `email`  (feature `email`):  verification + password-reset email sending

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub mod handlers;
pub mod session;

#[cfg(feature = "oauth")]
pub mod oauth;

#[cfg(feature = "email")]
pub mod email;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // NotAuthenticated reserved for future "must be logged in" handlers
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid {field}: {reason}")]
    InvalidInput { field: &'static str, reason: &'static str },
    #[error("conflict: {0} already in use")]
    Conflict(&'static str),
    #[error("not authenticated")]
    NotAuthenticated,
    #[error("storage error")]
    Storage(#[from] crate::db::StorageError),
    #[error("internal error")]
    Internal,
}

// ---- Password hashing ------------------------------------------------------

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| AuthError::Internal)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ---- Tokens ----------------------------------------------------------------

/// Generate a cryptographically random token. Returns (raw_token, sha256_hash).
/// The raw token is what the client sees (cookie or email link). Only the hash
/// is stored, so DB compromise can't impersonate active sessions.
pub fn new_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let hash = sha256_hex(&raw);
    (raw, hash)
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

// ---- Validation ------------------------------------------------------------

const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 30;
const PASSWORD_MIN: usize = 8;
const PASSWORD_MAX: usize = 128;

pub fn validate_username(s: &str) -> Result<(), AuthError> {
    if s.len() < USERNAME_MIN {
        return Err(AuthError::InvalidInput {
            field: "username",
            reason: "too short (min 3)",
        });
    }
    if s.len() > USERNAME_MAX {
        return Err(AuthError::InvalidInput {
            field: "username",
            reason: "too long (max 30)",
        });
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AuthError::InvalidInput {
            field: "username",
            reason: "only letters, digits, _ and - allowed",
        });
    }
    if !s.chars().next().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false) {
        return Err(AuthError::InvalidInput {
            field: "username",
            reason: "must start with a letter or digit",
        });
    }
    Ok(())
}

pub fn validate_password(s: &str) -> Result<(), AuthError> {
    if s.len() < PASSWORD_MIN {
        return Err(AuthError::InvalidInput {
            field: "password",
            reason: "too short (min 8)",
        });
    }
    if s.len() > PASSWORD_MAX {
        return Err(AuthError::InvalidInput {
            field: "password",
            reason: "too long (max 128)",
        });
    }
    Ok(())
}

pub fn validate_email(s: &str) -> Result<(), AuthError> {
    // Deliberately minimal: chess.com-shape sites get away with just
    // "has @ and a dot in the domain" because the verification email is the
    // real check. We don't try to be RFC-correct.
    let s = s.trim();
    if s.len() > 254 {
        return Err(AuthError::InvalidInput {
            field: "email",
            reason: "too long",
        });
    }
    let parts: Vec<&str> = s.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AuthError::InvalidInput {
            field: "email",
            reason: "missing @ or local/domain part",
        });
    }
    if !parts[1].contains('.') {
        return Err(AuthError::InvalidInput {
            field: "email",
            reason: "domain has no dot",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_verifies() {
        let h = hash_password("hunter2hunter2").unwrap();
        assert!(verify_password("hunter2hunter2", &h));
        assert!(!verify_password("wrongpass1234", &h));
    }

    #[test]
    fn token_is_unique() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a.0, b.0);
        assert_eq!(a.1.len(), 64); // sha256 hex
    }

    #[test]
    fn username_rules() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("alice_42").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("a".repeat(31).as_str()).is_err());
        assert!(validate_username("alice@bob").is_err());
        assert!(validate_username("_alice").is_err());
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("longenoughpw").is_ok());
    }

    #[test]
    fn email_rules() {
        assert!(validate_email("a@b.co").is_ok());
        assert!(validate_email("a@b").is_err());
        assert!(validate_email("@b.co").is_err());
        assert!(validate_email("a@").is_err());
    }
}
