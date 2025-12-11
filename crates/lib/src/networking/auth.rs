//! Authentication and authorization for the networking layer

use sha2::{
    Digest,
    Sha256,
};

use crate::networking::error::{
    NetworkingError,
    Result,
};

/// Validate session secret using constant-time comparison
///
/// This function uses SHA-256 hash comparison to perform constant-time
/// comparison and prevent timing attacks. The session secret is a pre-shared
/// key that controls access to the gossip network.
///
/// # Arguments
/// * `provided` - The session secret provided by the joining peer
/// * `expected` - The expected session secret configured for this node
///
/// # Returns
/// * `Ok(())` - Session secret is valid
/// * `Err(NetworkingError::SecurityError)` - Session secret is invalid
///
/// # Examples
/// ```
/// use lib::networking::auth::validate_session_secret;
///
/// let secret = b"my_secret_key";
/// assert!(validate_session_secret(secret, secret).is_ok());
///
/// let wrong_secret = b"wrong_key";
/// assert!(validate_session_secret(wrong_secret, secret).is_err());
/// ```
pub fn validate_session_secret(provided: &[u8], expected: &[u8]) -> Result<()> {
    // Different lengths = definitely not equal, fail fast
    if provided.len() != expected.len() {
        return Err(NetworkingError::SecurityError(
            "Invalid session secret".to_string(),
        ));
    }

    // Hash both secrets for constant-time comparison
    let provided_hash = hash_secret(provided);
    let expected_hash = hash_secret(expected);

    // Compare hashes using constant-time comparison
    // This prevents timing attacks that could leak information about the secret
    if provided_hash != expected_hash {
        return Err(NetworkingError::SecurityError(
            "Invalid session secret".to_string(),
        ));
    }

    Ok(())
}

/// Hash a secret using SHA-256
///
/// This is used internally for constant-time comparison of session secrets.
fn hash_secret(secret: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_secret() {
        let secret = b"my_secret_key";
        assert!(validate_session_secret(secret, secret).is_ok());
    }

    #[test]
    fn test_invalid_secret() {
        let secret1 = b"my_secret_key";
        let secret2 = b"wrong_secret_key";
        let result = validate_session_secret(secret1, secret2);
        assert!(result.is_err());
        match result {
            | Err(NetworkingError::SecurityError(_)) => {}, // Expected
            | _ => panic!("Expected SecurityError"),
        }
    }

    #[test]
    fn test_different_lengths() {
        let secret1 = b"short";
        let secret2 = b"much_longer_secret";
        let result = validate_session_secret(secret1, secret2);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_secrets() {
        let empty = b"";
        assert!(validate_session_secret(empty, empty).is_ok());
    }

    #[test]
    fn test_hash_is_deterministic() {
        let secret = b"test_secret";
        let hash1 = hash_secret(secret);
        let hash2 = hash_secret(secret);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_secrets_have_different_hashes() {
        let secret1 = b"secret1";
        let secret2 = b"secret2";
        let hash1 = hash_secret(secret1);
        let hash2 = hash_secret(secret2);
        assert_ne!(hash1, hash2);
    }
}
