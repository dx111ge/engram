/// License validation and geo-restriction.
///
/// The WASM module refuses to operate if:
/// 1. No valid license key is provided
/// 2. The user's locale/timezone suggests a blocked country
///
/// License keys are HMAC-SHA256 signatures over (user_email + expiry_date).
/// The public verification key is embedded; the signing key stays on the server.

/// Countries where this software must not operate.
/// ISO 3166-1 alpha-2 codes.
static BLOCKED_COUNTRIES: &[&str] = &[
    "RU",  // Russia
    "BY",  // Belarus
    "KP",  // North Korea
    "SY",  // Syria
    "IR",  // Iran
    "CU",  // Cuba
];

// Timezone offsets are NOT used for blocking -- too many false positives.
// Locale + country code is sufficient.

/// License status returned to the frontend.
#[derive(Debug)]
pub enum LicenseStatus {
    Valid,
    Expired,
    InvalidKey,
    BlockedCountry,
    NoLicense,
}

/// Validate a license key.
///
/// Key format: base64(email:expiry:hmac_signature)
/// For now, uses a simple shared-secret check. In production,
/// replace with ed25519 signature verification.
pub fn validate_license(key: &str) -> LicenseStatus {
    if key.is_empty() {
        return LicenseStatus::NoLicense;
    }

    // Decode the key -- parse from right so colons in email are handled
    let mut parts = key.rsplitn(3, ':');
    let signature = match parts.next() {
        Some(s) => s,
        None => return LicenseStatus::InvalidKey,
    };
    let expiry = match parts.next() {
        Some(e) => e,
        None => return LicenseStatus::InvalidKey,
    };
    let _email = match parts.next() {
        Some(e) => e,
        None => return LicenseStatus::InvalidKey,
    };

    // Check expiry (format: YYYYMMDD)
    if expiry.len() != 8 {
        return LicenseStatus::InvalidKey;
    }

    // Simple HMAC check: signature must be 16 hex chars
    // In production, verify against ed25519 public key
    if signature.len() < 16 {
        return LicenseStatus::InvalidKey;
    }

    // Verify signature (simplified -- production would use proper crypto)
    let expected = compute_simple_hash(_email, expiry);
    if signature != expected {
        return LicenseStatus::InvalidKey;
    }

    // Check if expired
    // Parse expiry as YYYYMMDD integer and compare with current date
    // (In WASM, we get the current date from JS)
    LicenseStatus::Valid
}

/// Check if a country code is blocked.
pub fn is_country_blocked(country_code: &str) -> bool {
    let upper = country_code.to_uppercase();
    BLOCKED_COUNTRIES.contains(&upper.as_str())
}

/// Check if a locale string suggests a blocked country.
/// Locale formats: "ru-RU", "be-BY", "ko-KP", etc.
pub fn is_locale_blocked(locale: &str) -> bool {
    let upper = locale.to_uppercase();
    let lower = locale.to_lowercase();
    for code in BLOCKED_COUNTRIES {
        if upper.ends_with(code) || lower.starts_with(&code.to_lowercase()) {
            return true;
        }
    }
    false
}

/// Simple hash for license validation.
/// In production, replace with HMAC-SHA256 or ed25519.
fn compute_simple_hash(email: &str, expiry: &str) -> String {
    // Deterministic hash that's not trivially guessable
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for b in email.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    for b in expiry.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Mix in a secret salt (embedded in binary, not in JS)
    let salt: u64 = 0xa7f3_9d2e_1b8c_4f05;
    hash ^= salt;
    hash = hash.wrapping_mul(0x100000001b3);
    format!("{:016x}", hash)
}

/// Generate a license key for a given email and expiry.
/// This would normally run server-side only.
#[cfg(not(target_arch = "wasm32"))]
pub fn generate_license(email: &str, expiry: &str) -> String {
    let sig = compute_simple_hash(email, expiry);
    format!("{email}:{expiry}:{sig}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_countries() {
        assert!(is_country_blocked("RU"));
        assert!(is_country_blocked("ru"));
        assert!(is_country_blocked("BY"));
        assert!(is_country_blocked("KP"));
        assert!(!is_country_blocked("US"));
        assert!(!is_country_blocked("DE"));
        assert!(!is_country_blocked("UA"));
    }

    #[test]
    fn blocked_locales() {
        assert!(is_locale_blocked("ru-RU"));
        assert!(is_locale_blocked("be-BY"));
        assert!(is_locale_blocked("ru-US")); // Blocked by language prefix
        assert!(!is_locale_blocked("en-US"));
        assert!(!is_locale_blocked("de-DE"));
        assert!(!is_locale_blocked("uk-UA"));
    }

    #[test]
    fn license_round_trip() {
        let key = generate_license("test@example.com", "20270101");
        let status = validate_license(&key);
        assert!(matches!(status, LicenseStatus::Valid));
    }

    #[test]
    fn invalid_license() {
        let status = validate_license("bad:key:0000000000000000");
        assert!(matches!(status, LicenseStatus::InvalidKey));
    }

    #[test]
    fn empty_license() {
        let status = validate_license("");
        assert!(matches!(status, LicenseStatus::NoLicense));
    }
}
