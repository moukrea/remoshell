//! QR Code generation for device pairing.
//!
//! This module provides QR code generation for device pairing, supporting both
//! terminal-based display using Unicode block characters and PNG file output.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{ImageBuffer, Luma};
use qrcode::QrCode;
use serde::{Deserialize, Serialize};

/// Encodes bytes as a base58 string.
///
/// Uses Bitcoin-style base58 alphabet (no 0, O, I, l).
fn to_base58(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    if bytes.is_empty() {
        return String::new();
    }

    // Count leading zeros
    let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();

    // Convert to base58
    let mut result = Vec::new();
    let mut num = bytes.to_vec();

    while !num.is_empty() && !num.iter().all(|&b| b == 0) {
        let mut carry = 0u32;
        for byte in num.iter_mut() {
            carry = carry * 256 + (*byte as u32);
            *byte = (carry / 58) as u8;
            carry %= 58;
        }
        result.push(ALPHABET[carry as usize]);

        // Remove leading zeros from num
        while !num.is_empty() && num[0] == 0 {
            num.remove(0);
        }
    }

    // Add leading '1's for each leading zero byte
    result.extend(std::iter::repeat(b'1').take(leading_zeros));

    result.reverse();
    String::from_utf8(result).unwrap()
}

/// Decodes a base58 string to bytes.
///
/// Uses Bitcoin-style base58 alphabet (no 0, O, I, l).
fn from_base58(s: &str) -> Result<Vec<u8>, &'static str> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    if s.is_empty() {
        return Ok(Vec::new());
    }

    // Count leading '1's
    let leading_ones = s.chars().take_while(|&c| c == '1').count();

    // Decode
    let mut result: Vec<u8> = Vec::new();
    for c in s.chars() {
        let idx = ALPHABET
            .iter()
            .position(|&b| b == c as u8)
            .ok_or("Invalid base58 character")?;

        let mut carry = idx as u32;
        for byte in result.iter_mut().rev() {
            carry += 58 * (*byte as u32);
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            result.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }

    // Add leading zeros for each leading '1'
    let mut final_result = vec![0u8; leading_ones];
    final_result.extend(result);

    Ok(final_result)
}

/// Default expiry duration in seconds (5 minutes).
pub const DEFAULT_EXPIRY_SECONDS: u64 = 300;

/// QR code module size in pixels for PNG output.
const PNG_MODULE_SIZE: u32 = 8;

/// Quiet zone (border) size in modules for PNG output.
const PNG_QUIET_ZONE: u32 = 4;

/// Information encoded in a pairing QR code.
///
/// This struct is serialized to JSON and encoded in the QR code. The scanning
/// device uses this information to establish a connection with the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingInfo {
    /// The device ID encoded as base58.
    pub device_id: String,

    /// The public key encoded as base64.
    pub public_key: String,

    /// The relay/signaling server URL.
    pub relay_url: String,

    /// Unix timestamp when this pairing code expires.
    pub expires: u64,
}

impl PairingInfo {
    /// Creates a new `PairingInfo` with the given parameters.
    ///
    /// # Arguments
    /// * `device_id` - The device ID as base58 string.
    /// * `public_key` - The public key as raw bytes (32 bytes for Ed25519).
    /// * `relay_url` - The relay/signaling server URL.
    /// * `expiry_seconds` - Number of seconds until expiry. Use `None` for default (5 minutes).
    ///
    /// # Returns
    /// A new `PairingInfo` instance with the expiry timestamp set.
    pub fn new(
        device_id: String,
        public_key: &[u8],
        relay_url: String,
        expiry_seconds: Option<u64>,
    ) -> Self {
        let expiry = expiry_seconds.unwrap_or(DEFAULT_EXPIRY_SECONDS);
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            + expiry;

        Self {
            device_id,
            public_key: BASE64.encode(public_key),
            relay_url,
            expires,
        }
    }

    /// Creates a `PairingInfo` from a device identity.
    ///
    /// # Arguments
    /// * `identity` - The device identity.
    /// * `relay_url` - The relay/signaling server URL.
    /// * `expiry_seconds` - Number of seconds until expiry. Use `None` for default (5 minutes).
    pub fn from_identity(
        identity: &protocol::DeviceIdentity,
        relay_url: String,
        expiry_seconds: Option<u64>,
    ) -> Self {
        Self::new(
            to_base58(identity.device_id().as_bytes()),
            &identity.public_key_bytes(),
            relay_url,
            expiry_seconds,
        )
    }

    /// Returns the device ID as bytes, decoded from base58.
    ///
    /// # Errors
    /// Returns an error if the device_id is not valid base58.
    pub fn device_id_bytes(&self) -> Result<Vec<u8>, &'static str> {
        from_base58(&self.device_id)
    }

    /// Checks if this pairing info has expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        now > self.expires
    }

    /// Returns the number of seconds until expiry, or 0 if already expired.
    pub fn seconds_until_expiry(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        self.expires.saturating_sub(now)
    }

    /// Serializes this pairing info to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes pairing info from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Generates a terminal-displayable QR code using Unicode block characters.
///
/// The QR code uses Unicode block characters to render a scannable QR code
/// in the terminal:
/// - Upper half block (U+2580): represents a dark module on top, light below
/// - Lower half block (U+2584): represents a light module on top, dark below
/// - Full block (U+2588): represents two dark modules
/// - Space: represents two light modules
///
/// # Arguments
/// * `info` - The pairing information to encode.
///
/// # Returns
/// A string containing the terminal-renderable QR code.
///
/// # Errors
/// Returns an error if the QR code cannot be generated.
pub fn generate_terminal_qr(info: &PairingInfo) -> anyhow::Result<String> {
    let json = info.to_json()?;
    let code = QrCode::new(json.as_bytes())?;
    let modules = code.to_colors();
    let width = code.width();

    // We process two rows at a time, using half-block characters
    // to fit more information in the terminal
    let mut output = String::new();

    // Add quiet zone (top border)
    let full_width = width + 8; // 4 modules quiet zone on each side
    for _ in 0..4 {
        output.push_str(&" ".repeat(full_width));
        output.push('\n');
    }

    // Process modules two rows at a time
    let height = modules.len() / width;
    let mut row = 0;
    while row < height {
        // Add left quiet zone
        output.push_str("    ");

        for col in 0..width {
            let top_dark = modules[row * width + col] == qrcode::Color::Dark;
            let bottom_dark = if row + 1 < height {
                modules[(row + 1) * width + col] == qrcode::Color::Dark
            } else {
                false
            };

            // Unicode block characters for terminal QR
            // The terminal typically has light background, so:
            // - Dark modules = black (full blocks)
            // - Light modules = space (background shows)
            //
            // Using inverted colors for better visibility on most terminals:
            let ch = match (top_dark, bottom_dark) {
                (true, true) => '\u{2588}',  // Full block (both dark)
                (true, false) => '\u{2580}', // Upper half block
                (false, true) => '\u{2584}', // Lower half block
                (false, false) => ' ',       // Space (both light)
            };
            output.push(ch);
        }

        // Add right quiet zone
        output.push_str("    ");
        output.push('\n');
        row += 2;
    }

    // Add quiet zone (bottom border)
    for _ in 0..4 {
        output.push_str(&" ".repeat(full_width));
        output.push('\n');
    }

    Ok(output)
}

/// Generates an inverted terminal QR code (dark background).
///
/// This version uses inverted colors which may be more visible on
/// terminals with light backgrounds.
///
/// # Arguments
/// * `info` - The pairing information to encode.
///
/// # Returns
/// A string containing the terminal-renderable QR code.
///
/// # Errors
/// Returns an error if the QR code cannot be generated.
pub fn generate_terminal_qr_inverted(info: &PairingInfo) -> anyhow::Result<String> {
    let json = info.to_json()?;
    let code = QrCode::new(json.as_bytes())?;
    let modules = code.to_colors();
    let width = code.width();

    let mut output = String::new();

    // Add quiet zone (top border) - use full blocks for dark border
    let full_width = width + 8;
    for _ in 0..4 {
        output.push_str(&"\u{2588}".repeat(full_width));
        output.push('\n');
    }

    // Process modules two rows at a time
    let height = modules.len() / width;
    let mut row = 0;
    while row < height {
        // Add left quiet zone (dark)
        output.push_str("\u{2588}\u{2588}\u{2588}\u{2588}");

        for col in 0..width {
            // Invert the logic: dark modules become light, light become dark
            let top_light = modules[row * width + col] == qrcode::Color::Dark;
            let bottom_light = if row + 1 < height {
                modules[(row + 1) * width + col] == qrcode::Color::Dark
            } else {
                true // Treat out of bounds as dark (which becomes light when inverted)
            };

            // Inverted colors
            let ch = match (top_light, bottom_light) {
                (true, true) => ' ',          // Both "light" (was dark)
                (true, false) => '\u{2584}',  // Lower half block
                (false, true) => '\u{2580}',  // Upper half block
                (false, false) => '\u{2588}', // Full block (both "dark")
            };
            output.push(ch);
        }

        // Add right quiet zone (dark)
        output.push_str("\u{2588}\u{2588}\u{2588}\u{2588}");
        output.push('\n');
        row += 2;
    }

    // Add quiet zone (bottom border) - use full blocks for dark border
    for _ in 0..4 {
        output.push_str(&"\u{2588}".repeat(full_width));
        output.push('\n');
    }

    Ok(output)
}

/// Generates a PNG QR code and saves it to the specified path.
///
/// # Arguments
/// * `info` - The pairing information to encode.
/// * `path` - The file path where the PNG will be saved.
///
/// # Errors
/// Returns an error if the QR code cannot be generated or saved.
pub fn generate_png_qr(info: &PairingInfo, path: &Path) -> anyhow::Result<()> {
    let json = info.to_json()?;
    let code = QrCode::new(json.as_bytes())?;
    let modules = code.to_colors();
    let qr_width = code.width();

    // Calculate image dimensions
    let quiet_zone_pixels = PNG_QUIET_ZONE * PNG_MODULE_SIZE;
    let qr_pixels = qr_width as u32 * PNG_MODULE_SIZE;
    let image_size = qr_pixels + 2 * quiet_zone_pixels;

    // Create a grayscale image buffer
    let mut img: ImageBuffer<Luma<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(image_size, image_size, Luma([255u8])); // White background

    // Draw the QR code modules
    for (idx, color) in modules.iter().enumerate() {
        let row = (idx / qr_width) as u32;
        let col = (idx % qr_width) as u32;

        let pixel_color = if *color == qrcode::Color::Dark {
            Luma([0u8]) // Black
        } else {
            Luma([255u8]) // White
        };

        // Draw the module as a square
        let x_start = quiet_zone_pixels + col * PNG_MODULE_SIZE;
        let y_start = quiet_zone_pixels + row * PNG_MODULE_SIZE;

        for dy in 0..PNG_MODULE_SIZE {
            for dx in 0..PNG_MODULE_SIZE {
                img.put_pixel(x_start + dx, y_start + dy, pixel_color);
            }
        }
    }

    // Save the image
    img.save(path)?;

    Ok(())
}

/// Generates a PNG QR code and returns it as bytes.
///
/// # Arguments
/// * `info` - The pairing information to encode.
///
/// # Returns
/// The PNG image as a byte vector.
///
/// # Errors
/// Returns an error if the QR code cannot be generated.
pub fn generate_png_qr_bytes(info: &PairingInfo) -> anyhow::Result<Vec<u8>> {
    use std::io::Cursor;

    let json = info.to_json()?;
    let code = QrCode::new(json.as_bytes())?;
    let modules = code.to_colors();
    let qr_width = code.width();

    // Calculate image dimensions
    let quiet_zone_pixels = PNG_QUIET_ZONE * PNG_MODULE_SIZE;
    let qr_pixels = qr_width as u32 * PNG_MODULE_SIZE;
    let image_size = qr_pixels + 2 * quiet_zone_pixels;

    // Create a grayscale image buffer
    let mut img: ImageBuffer<Luma<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(image_size, image_size, Luma([255u8]));

    // Draw the QR code modules
    for (idx, color) in modules.iter().enumerate() {
        let row = (idx / qr_width) as u32;
        let col = (idx % qr_width) as u32;

        let pixel_color = if *color == qrcode::Color::Dark {
            Luma([0u8])
        } else {
            Luma([255u8])
        };

        let x_start = quiet_zone_pixels + col * PNG_MODULE_SIZE;
        let y_start = quiet_zone_pixels + row * PNG_MODULE_SIZE;

        for dy in 0..PNG_MODULE_SIZE {
            for dx in 0..PNG_MODULE_SIZE {
                img.put_pixel(x_start + dx, y_start + dy, pixel_color);
            }
        }
    }

    // Encode to PNG bytes
    let mut bytes = Vec::new();
    let mut cursor = Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png)?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_info_creation() {
        let public_key = [1u8; 32];
        let info = PairingInfo::new(
            "TestDevice123".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        assert_eq!(info.device_id, "TestDevice123");
        assert_eq!(info.public_key, BASE64.encode(&public_key));
        assert_eq!(info.relay_url, "wss://relay.example.com");
        assert!(!info.is_expired());
        assert!(info.seconds_until_expiry() > 0);
        assert!(info.seconds_until_expiry() <= 300);
    }

    #[test]
    fn test_pairing_info_from_identity() {
        let identity = protocol::DeviceIdentity::generate();
        let info =
            PairingInfo::from_identity(&identity, "wss://relay.example.com".to_string(), Some(60));

        assert_eq!(info.device_id, to_base58(identity.device_id().as_bytes()));
        assert_eq!(info.public_key, BASE64.encode(&identity.public_key_bytes()));
        assert!(!info.is_expired());
    }

    #[test]
    fn test_pairing_info_default_expiry() {
        let public_key = [0u8; 32];
        let info = PairingInfo::new(
            "device".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            None, // Use default expiry
        );

        // Should use DEFAULT_EXPIRY_SECONDS (300 seconds = 5 minutes)
        let seconds = info.seconds_until_expiry();
        assert!(seconds > 290 && seconds <= 300);
    }

    #[test]
    fn test_pairing_info_expired() {
        let public_key = [0u8; 32];
        let mut info = PairingInfo::new(
            "device".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(1),
        );

        // Manually set expiry to the past
        info.expires = 0;
        assert!(info.is_expired());
        assert_eq!(info.seconds_until_expiry(), 0);
    }

    #[test]
    fn test_pairing_info_serialization() {
        let public_key = [42u8; 32];
        let info = PairingInfo::new(
            "Device123".to_string(),
            &public_key,
            "wss://relay.test.com".to_string(),
            Some(600),
        );

        let json = info.to_json().expect("Failed to serialize");
        let parsed: PairingInfo = PairingInfo::from_json(&json).expect("Failed to deserialize");

        assert_eq!(info, parsed);
    }

    #[test]
    fn test_pairing_info_json_format() {
        let public_key = [0u8; 32];
        let info = PairingInfo {
            device_id: "TestDeviceId".to_string(),
            public_key: BASE64.encode(&public_key),
            relay_url: "wss://relay.example.com".to_string(),
            expires: 1234567890,
        };

        let json = info.to_json().expect("Failed to serialize");

        // Verify JSON structure
        assert!(json.contains("\"device_id\":\"TestDeviceId\""));
        assert!(json.contains("\"relay_url\":\"wss://relay.example.com\""));
        assert!(json.contains("\"expires\":1234567890"));
        assert!(json.contains("\"public_key\":"));
    }

    #[test]
    fn test_terminal_qr_generation() {
        let public_key = [1u8; 32];
        let info = PairingInfo::new(
            "TestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        let qr = generate_terminal_qr(&info).expect("Failed to generate terminal QR");

        // QR should contain some content
        assert!(!qr.is_empty());

        // QR should have multiple lines
        let lines: Vec<&str> = qr.lines().collect();
        assert!(lines.len() > 10, "QR code should have multiple rows");

        // QR should contain Unicode block characters
        assert!(
            qr.contains('\u{2588}') || qr.contains('\u{2580}') || qr.contains('\u{2584}'),
            "QR code should contain Unicode block characters"
        );
    }

    #[test]
    fn test_terminal_qr_inverted_generation() {
        let public_key = [1u8; 32];
        let info = PairingInfo::new(
            "TestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        let qr = generate_terminal_qr_inverted(&info).expect("Failed to generate inverted QR");

        // QR should contain some content
        assert!(!qr.is_empty());

        // Inverted QR should have full blocks in the border
        let lines: Vec<&str> = qr.lines().collect();
        assert!(!lines.is_empty());
        // First line should be all full blocks (quiet zone)
        assert!(
            lines[0].chars().all(|c| c == '\u{2588}'),
            "Top border should be full blocks in inverted mode"
        );
    }

    #[test]
    fn test_png_qr_generation() {
        use tempfile::TempDir;

        let public_key = [2u8; 32];
        let info = PairingInfo::new(
            "PNGTestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("test_qr.png");

        generate_png_qr(&info, &path).expect("Failed to generate PNG QR");

        // Verify file was created
        assert!(path.exists(), "PNG file should be created");

        // Verify file has content
        let metadata = std::fs::metadata(&path).expect("Failed to get metadata");
        assert!(metadata.len() > 0, "PNG file should have content");

        // Verify it's a valid PNG by checking the header
        let file_bytes = std::fs::read(&path).expect("Failed to read file");
        assert!(
            file_bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]),
            "File should have PNG header"
        );
    }

    #[test]
    fn test_png_qr_bytes_generation() {
        let public_key = [3u8; 32];
        let info = PairingInfo::new(
            "BytesTestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        let bytes = generate_png_qr_bytes(&info).expect("Failed to generate PNG bytes");

        // Verify bytes have content
        assert!(!bytes.is_empty(), "PNG bytes should have content");

        // Verify it's a valid PNG by checking the header
        assert!(
            bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]),
            "Bytes should have PNG header"
        );
    }

    #[test]
    fn test_qr_content_is_parseable() {
        let public_key = [4u8; 32];
        let original_info = PairingInfo::new(
            "ParseableDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        // Get the JSON content that would be encoded in the QR
        let json = original_info.to_json().expect("Failed to serialize");

        // Verify it can be parsed back
        let parsed_info: PairingInfo = serde_json::from_str(&json).expect("Failed to parse JSON");

        assert_eq!(original_info.device_id, parsed_info.device_id);
        assert_eq!(original_info.public_key, parsed_info.public_key);
        assert_eq!(original_info.relay_url, parsed_info.relay_url);
        assert_eq!(original_info.expires, parsed_info.expires);
    }

    #[test]
    fn test_expiry_encoding() {
        let public_key = [5u8; 32];

        // Test with specific expiry
        let info = PairingInfo::new(
            "ExpiryTestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(120), // 2 minutes
        );

        let json = info.to_json().expect("Failed to serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");

        // Verify expires field is a number
        assert!(parsed["expires"].is_u64(), "expires should be a u64");

        // Verify the expiry is in the future
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let expires = parsed["expires"].as_u64().unwrap();
        assert!(expires > now, "expires should be in the future");
        assert!(expires <= now + 120, "expires should be within 2 minutes");
    }

    #[test]
    fn test_public_key_base64_encoding() {
        let public_key: [u8; 32] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ];

        let info = PairingInfo::new(
            "Base64TestDevice".to_string(),
            &public_key,
            "wss://relay.example.com".to_string(),
            Some(300),
        );

        // Decode the base64 and verify it matches
        let decoded = BASE64
            .decode(&info.public_key)
            .expect("Failed to decode base64");
        assert_eq!(
            decoded, public_key,
            "Decoded public key should match original"
        );
    }

    #[test]
    fn test_device_id_base58_from_identity() {
        let identity = protocol::DeviceIdentity::generate();
        let info =
            PairingInfo::from_identity(&identity, "wss://relay.example.com".to_string(), None);

        // Verify the device ID can be parsed back from base58
        let device_id_bytes = info
            .device_id_bytes()
            .expect("Failed to parse device ID from base58");
        assert_eq!(&device_id_bytes[..], identity.device_id().as_bytes());
    }

    #[test]
    fn test_qr_code_size_reasonable() {
        // Test that the QR code can handle the typical content size
        let identity = protocol::DeviceIdentity::generate();
        let info = PairingInfo::from_identity(
            &identity,
            "wss://signaling.remoshell.io:8443".to_string(),
            Some(300),
        );

        let json = info.to_json().expect("Failed to serialize");

        // The JSON should be reasonable size for QR encoding
        assert!(
            json.len() < 500,
            "JSON content should fit in a QR code: {} bytes",
            json.len()
        );

        // Should be able to generate both types of QR
        let terminal_qr = generate_terminal_qr(&info);
        assert!(terminal_qr.is_ok(), "Should generate terminal QR");

        let png_bytes = generate_png_qr_bytes(&info);
        assert!(png_bytes.is_ok(), "Should generate PNG QR");
    }
}
