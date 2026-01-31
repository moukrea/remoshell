//! Frame codec for length-prefixed framing with optional LZ4 compression.
//!
//! # Frame Format
//!
//! Each frame consists of:
//! - 4 bytes: magic bytes "RMSH"
//! - 4 bytes: payload length (big-endian, includes flags byte)
//! - 1 byte: flags (bit 0 = compressed)
//! - N bytes: payload (possibly LZ4 compressed)
//!
//! # Compression
//!
//! Payloads larger than 1KB are automatically compressed using LZ4.
//! The compressed flag in the frame header indicates whether the payload
//! is compressed.

use crate::error::{ProtocolError, Result};

/// Magic bytes identifying a RemoShell frame.
pub const FRAME_MAGIC: [u8; 4] = *b"RMSH";

/// Compression threshold in bytes. Payloads larger than this are compressed.
pub const COMPRESSION_THRESHOLD: usize = 1024;

/// Maximum frame size (16 MB).
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Frame header size: 4 (magic) + 4 (length) + 1 (flags) = 9 bytes.
pub const FRAME_HEADER_SIZE: usize = 9;

/// Flags indicating frame properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameFlags(u8);

impl FrameFlags {
    /// Flag indicating the payload is LZ4 compressed.
    pub const COMPRESSED: u8 = 0b0000_0001;

    /// Create a new empty flags set.
    #[inline]
    pub fn new() -> Self {
        Self(0)
    }

    /// Create flags from a raw byte value.
    #[inline]
    pub fn from_byte(byte: u8) -> Self {
        Self(byte)
    }

    /// Get the raw byte value of the flags.
    #[inline]
    pub fn as_byte(self) -> u8 {
        self.0
    }

    /// Check if the compressed flag is set.
    #[inline]
    pub fn is_compressed(self) -> bool {
        self.0 & Self::COMPRESSED != 0
    }

    /// Set the compressed flag.
    #[inline]
    pub fn set_compressed(&mut self, compressed: bool) {
        if compressed {
            self.0 |= Self::COMPRESSED;
        } else {
            self.0 &= !Self::COMPRESSED;
        }
    }

    /// Return a new flags with compressed set.
    #[inline]
    pub fn with_compressed(mut self, compressed: bool) -> Self {
        self.set_compressed(compressed);
        self
    }
}

/// A frame containing a header and payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Frame flags.
    pub flags: FrameFlags,
    /// The payload data (uncompressed form).
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame with the given payload.
    pub fn new(payload: Vec<u8>) -> Self {
        Self {
            flags: FrameFlags::new(),
            payload,
        }
    }

    /// Create a new frame with explicit flags.
    pub fn with_flags(payload: Vec<u8>, flags: FrameFlags) -> Self {
        Self { flags, payload }
    }
}

/// Encoder and decoder for frames.
#[derive(Debug, Clone, Default)]
pub struct FrameCodec {
    /// Whether to enable compression for large payloads.
    compression_enabled: bool,
}

impl FrameCodec {
    /// Create a new frame codec with compression enabled.
    pub fn new() -> Self {
        Self {
            compression_enabled: true,
        }
    }

    /// Create a new frame codec with compression disabled.
    pub fn without_compression() -> Self {
        Self {
            compression_enabled: false,
        }
    }

    /// Enable or disable compression.
    pub fn set_compression(&mut self, enabled: bool) {
        self.compression_enabled = enabled;
    }

    /// Encode a frame into bytes.
    ///
    /// The frame format is:
    /// - 4 bytes: magic "RMSH"
    /// - 4 bytes: length of (flags + payload) in big-endian
    /// - 1 byte: flags
    /// - N bytes: payload (possibly compressed)
    pub fn encode(&self, frame: &Frame) -> Result<Vec<u8>> {
        let payload = &frame.payload;

        // Check payload size before any processing
        if payload.len() > MAX_FRAME_SIZE - FRAME_HEADER_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: payload.len() + FRAME_HEADER_SIZE,
                max: MAX_FRAME_SIZE,
            });
        }

        // Determine if we should compress
        let should_compress = self.compression_enabled && payload.len() > COMPRESSION_THRESHOLD;

        let (encoded_payload, flags) = if should_compress {
            let compressed = lz4_flex::compress_prepend_size(payload);
            // Only use compression if it actually reduces size
            if compressed.len() < payload.len() {
                (compressed, FrameFlags::new().with_compressed(true))
            } else {
                (payload.clone(), frame.flags)
            }
        } else {
            (payload.clone(), frame.flags)
        };

        // Calculate total length (flags byte + payload)
        let content_len = 1 + encoded_payload.len();

        // Check final frame size
        let total_size = FRAME_HEADER_SIZE - 1 + content_len; // -1 because content_len includes flags
        if total_size > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: total_size,
                max: MAX_FRAME_SIZE,
            });
        }

        // Build the frame
        let mut output = Vec::with_capacity(total_size);

        // Magic bytes
        output.extend_from_slice(&FRAME_MAGIC);

        // Length (big-endian u32)
        output.extend_from_slice(&(content_len as u32).to_be_bytes());

        // Flags
        output.push(flags.as_byte());

        // Payload
        output.extend_from_slice(&encoded_payload);

        Ok(output)
    }

    /// Decode a frame from bytes.
    ///
    /// Returns the decoded frame and the number of bytes consumed.
    pub fn decode(&self, data: &[u8]) -> Result<(Frame, usize)> {
        // Check minimum size
        if data.len() < FRAME_HEADER_SIZE {
            return Err(ProtocolError::Deserialization(format!(
                "insufficient data for frame header: need {} bytes, have {}",
                FRAME_HEADER_SIZE,
                data.len()
            )));
        }

        // Validate magic bytes
        let magic = &data[0..4];
        if magic != FRAME_MAGIC {
            // Convert to u32 for error display purposes
            let expected = u32::from_be_bytes(FRAME_MAGIC);
            let got = u32::from_be_bytes([magic[0], magic[1], magic[2], magic[3]]);
            return Err(ProtocolError::Deserialization(format!(
                "invalid frame magic: expected 0x{:08x} (RMSH), got 0x{:08x}",
                expected, got
            )));
        }

        // Read length
        let length_bytes: [u8; 4] = data[4..8].try_into().unwrap();
        let content_len = u32::from_be_bytes(length_bytes) as usize;

        // Check for oversized frames
        let total_frame_size = 8 + content_len; // magic + length + content
        if total_frame_size > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: total_frame_size,
                max: MAX_FRAME_SIZE,
            });
        }

        // Check we have enough data
        if data.len() < 8 + content_len {
            return Err(ProtocolError::Deserialization(format!(
                "insufficient data for frame: need {} bytes, have {}",
                8 + content_len,
                data.len()
            )));
        }

        // Content must have at least the flags byte
        if content_len < 1 {
            return Err(ProtocolError::Deserialization(
                "invalid frame: content length must be at least 1 for flags byte".to_string(),
            ));
        }

        // Read flags
        let flags = FrameFlags::from_byte(data[8]);

        // Read payload
        let payload_data = &data[9..8 + content_len];

        // Decompress if needed
        let payload = if flags.is_compressed() {
            lz4_flex::decompress_size_prepended(payload_data).map_err(|e| {
                ProtocolError::Deserialization(format!("failed to decompress payload: {}", e))
            })?
        } else {
            payload_data.to_vec()
        };

        let frame = Frame {
            flags: FrameFlags::new(), // Clear compression flag since payload is now decompressed
            payload,
        };

        Ok((frame, 8 + content_len))
    }

    /// Try to decode a frame from bytes, returning None if there isn't enough data.
    ///
    /// This is useful for streaming scenarios where you may receive partial frames.
    pub fn try_decode(&self, data: &[u8]) -> Result<Option<(Frame, usize)>> {
        // Check minimum size
        if data.len() < FRAME_HEADER_SIZE {
            return Ok(None);
        }

        // Validate magic bytes
        let magic = &data[0..4];
        if magic != FRAME_MAGIC {
            let expected = u32::from_be_bytes(FRAME_MAGIC);
            let got = u32::from_be_bytes([magic[0], magic[1], magic[2], magic[3]]);
            return Err(ProtocolError::Deserialization(format!(
                "invalid frame magic: expected 0x{:08x} (RMSH), got 0x{:08x}",
                expected, got
            )));
        }

        // Read length
        let length_bytes: [u8; 4] = data[4..8].try_into().unwrap();
        let content_len = u32::from_be_bytes(length_bytes) as usize;

        // Check for oversized frames
        let total_frame_size = 8 + content_len;
        if total_frame_size > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: total_frame_size,
                max: MAX_FRAME_SIZE,
            });
        }

        // Check we have enough data
        if data.len() < 8 + content_len {
            return Ok(None);
        }

        // We have enough data, do full decode
        self.decode(data).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_flags_default() {
        let flags = FrameFlags::new();
        assert_eq!(flags.as_byte(), 0);
        assert!(!flags.is_compressed());
    }

    #[test]
    fn test_frame_flags_compressed() {
        let mut flags = FrameFlags::new();
        flags.set_compressed(true);
        assert!(flags.is_compressed());
        assert_eq!(flags.as_byte(), 0b0000_0001);

        flags.set_compressed(false);
        assert!(!flags.is_compressed());
        assert_eq!(flags.as_byte(), 0);
    }

    #[test]
    fn test_frame_flags_with_compressed() {
        let flags = FrameFlags::new().with_compressed(true);
        assert!(flags.is_compressed());
    }

    #[test]
    fn test_frame_flags_from_byte() {
        let flags = FrameFlags::from_byte(0b0000_0001);
        assert!(flags.is_compressed());

        let flags = FrameFlags::from_byte(0b1111_1110);
        assert!(!flags.is_compressed());
    }

    #[test]
    fn test_frame_new() {
        let payload = vec![1, 2, 3, 4, 5];
        let frame = Frame::new(payload.clone());
        assert_eq!(frame.payload, payload);
        assert!(!frame.flags.is_compressed());
    }

    #[test]
    fn test_encode_decode_roundtrip_small() {
        let codec = FrameCodec::new();
        let original = Frame::new(vec![1, 2, 3, 4, 5]);

        let encoded = codec.encode(&original).unwrap();
        let (decoded, consumed) = codec.decode(&encoded).unwrap();

        assert_eq!(decoded.payload, original.payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_encode_decode_roundtrip_empty() {
        let codec = FrameCodec::new();
        let original = Frame::new(vec![]);

        let encoded = codec.encode(&original).unwrap();
        let (decoded, consumed) = codec.decode(&encoded).unwrap();

        assert_eq!(decoded.payload, original.payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_encode_decode_roundtrip_large_compressed() {
        let codec = FrameCodec::new();
        // Create a payload larger than compression threshold
        // Use repetitive data that compresses well
        let payload: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
        let original = Frame::new(payload);

        let encoded = codec.encode(&original).unwrap();

        // Verify compression was applied (encoded should be smaller for repetitive data)
        // Check that the flags byte indicates compression
        assert_eq!(encoded[8] & 0x01, 0x01, "compression flag should be set");

        let (decoded, consumed) = codec.decode(&encoded).unwrap();

        assert_eq!(decoded.payload, original.payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_encode_without_compression() {
        let codec = FrameCodec::without_compression();
        let payload: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
        let original = Frame::new(payload);

        let encoded = codec.encode(&original).unwrap();

        // Verify compression was NOT applied
        assert_eq!(
            encoded[8] & 0x01,
            0x00,
            "compression flag should not be set"
        );

        let (decoded, _) = codec.decode(&encoded).unwrap();
        assert_eq!(decoded.payload, original.payload);
    }

    #[test]
    fn test_compression_threshold() {
        let codec = FrameCodec::new();

        // Payload at threshold - should NOT be compressed
        let at_threshold = Frame::new(vec![0u8; COMPRESSION_THRESHOLD]);
        let encoded = codec.encode(&at_threshold).unwrap();
        assert_eq!(
            encoded[8] & 0x01,
            0x00,
            "payload at threshold should not be compressed"
        );

        // Payload above threshold - should be compressed
        let above_threshold = Frame::new(vec![0u8; COMPRESSION_THRESHOLD + 1]);
        let encoded = codec.encode(&above_threshold).unwrap();
        assert_eq!(
            encoded[8] & 0x01,
            0x01,
            "payload above threshold should be compressed"
        );
    }

    #[test]
    fn test_magic_bytes_validation() {
        let codec = FrameCodec::new();

        // Create a frame with wrong magic bytes
        let mut bad_frame = vec![b'B', b'A', b'D', b'!'];
        bad_frame.extend_from_slice(&5u32.to_be_bytes()); // length
        bad_frame.push(0); // flags
        bad_frame.extend_from_slice(&[1, 2, 3, 4]); // payload

        let result = codec.decode(&bad_frame);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("invalid frame magic"),
            "error should mention invalid magic: {}",
            err
        );
    }

    #[test]
    fn test_frame_too_large() {
        let codec = FrameCodec::without_compression();

        // Create a frame that exceeds max size
        let large_payload = vec![0u8; MAX_FRAME_SIZE];
        let frame = Frame::new(large_payload);

        let result = codec.encode(&frame);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
    }

    #[test]
    fn test_decode_oversized_length() {
        let codec = FrameCodec::new();

        // Create a frame header that claims a huge size
        let mut bad_frame = Vec::new();
        bad_frame.extend_from_slice(&FRAME_MAGIC);
        bad_frame.extend_from_slice(&(MAX_FRAME_SIZE as u32 + 1).to_be_bytes());
        bad_frame.push(0); // flags

        let result = codec.decode(&bad_frame);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
    }

    #[test]
    fn test_decode_insufficient_header() {
        let codec = FrameCodec::new();

        let short_data = vec![b'R', b'M', b'S']; // Only 3 bytes
        let result = codec.decode(&short_data);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("insufficient data"));
    }

    #[test]
    fn test_decode_insufficient_payload() {
        let codec = FrameCodec::new();

        // Header says 100 bytes of content, but we only have header
        let mut short_frame = Vec::new();
        short_frame.extend_from_slice(&FRAME_MAGIC);
        short_frame.extend_from_slice(&100u32.to_be_bytes());
        short_frame.push(0); // flags

        let result = codec.decode(&short_frame);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("insufficient data"));
    }

    #[test]
    fn test_try_decode_partial_data() {
        let codec = FrameCodec::new();
        let original = Frame::new(vec![1, 2, 3, 4, 5]);

        let encoded = codec.encode(&original).unwrap();

        // Try decoding with partial data - should return None
        for i in 0..encoded.len() - 1 {
            let result = codec.try_decode(&encoded[..i]).unwrap();
            assert!(
                result.is_none(),
                "should return None for partial data (len={})",
                i
            );
        }

        // Full data should succeed
        let result = codec.try_decode(&encoded).unwrap();
        assert!(result.is_some());
        let (decoded, consumed) = result.unwrap();
        assert_eq!(decoded.payload, original.payload);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_try_decode_invalid_magic() {
        let codec = FrameCodec::new();

        let mut bad_frame = vec![b'B', b'A', b'D', b'!'];
        bad_frame.extend_from_slice(&5u32.to_be_bytes());
        bad_frame.push(0);
        bad_frame.extend_from_slice(&[1, 2, 3, 4]);

        // Invalid magic should return error, not None
        let result = codec.try_decode(&bad_frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_header_format() {
        let codec = FrameCodec::new();
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let frame = Frame::new(payload.clone());

        let encoded = codec.encode(&frame).unwrap();

        // Check magic bytes
        assert_eq!(&encoded[0..4], b"RMSH");

        // Check length (1 byte flags + 4 byte payload = 5)
        let length = u32::from_be_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]);
        assert_eq!(length, 5);

        // Check flags (no compression for small payload)
        assert_eq!(encoded[8], 0);

        // Check payload
        assert_eq!(&encoded[9..], &payload[..]);
    }

    #[test]
    fn test_decode_corrupted_compressed_data() {
        let codec = FrameCodec::new();

        // Create a frame that claims to be compressed but has garbage data
        let mut bad_frame = Vec::new();
        bad_frame.extend_from_slice(&FRAME_MAGIC);
        bad_frame.extend_from_slice(&10u32.to_be_bytes()); // length
        bad_frame.push(0x01); // compressed flag
        bad_frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

        let result = codec.decode(&bad_frame);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("decompress"));
    }

    #[test]
    fn test_multiple_frames_in_buffer() {
        let codec = FrameCodec::new();
        let frame1 = Frame::new(vec![1, 2, 3]);
        let frame2 = Frame::new(vec![4, 5, 6, 7]);

        let encoded1 = codec.encode(&frame1).unwrap();
        let encoded2 = codec.encode(&frame2).unwrap();

        let mut combined = encoded1.clone();
        combined.extend_from_slice(&encoded2);

        // Decode first frame
        let (decoded1, consumed1) = codec.decode(&combined).unwrap();
        assert_eq!(decoded1.payload, frame1.payload);
        assert_eq!(consumed1, encoded1.len());

        // Decode second frame from remaining data
        let (decoded2, consumed2) = codec.decode(&combined[consumed1..]).unwrap();
        assert_eq!(decoded2.payload, frame2.payload);
        assert_eq!(consumed2, encoded2.len());
    }

    #[test]
    fn test_compression_not_used_when_not_beneficial() {
        let codec = FrameCodec::new();

        // Random-ish data that doesn't compress well
        let payload: Vec<u8> = (0..2048).map(|i| ((i * 17 + 31) % 256) as u8).collect();
        let original = Frame::new(payload.clone());

        let encoded = codec.encode(&original).unwrap();
        let (decoded, _) = codec.decode(&encoded).unwrap();

        // Regardless of whether compression was used, roundtrip should work
        assert_eq!(decoded.payload, original.payload);
    }
}
