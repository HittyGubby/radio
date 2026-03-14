use serde::{Deserialize, Serialize};

/// Spectrogram data packet sent over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrogramPacket {
    /// Timestamp in milliseconds
    pub timestamp: u64,
    /// Multiple frequency data frames (each frame is 0-255, representing dB levels)
    pub frames: Vec<Vec<u8>>,
}

impl SpectrogramPacket {
    /// Create a new spectrogram packet with a single frame
    pub fn new(timestamp: u64, frequency_data: Vec<u8>) -> Self {
        Self {
            timestamp,
            frames: vec![frequency_data],
        }
    }

    /// Create a new spectrogram packet with multiple frames
    pub fn new_with_frames(timestamp: u64, frames: Vec<Vec<u8>>) -> Self {
        Self {
            timestamp,
            frames,
        }
    }

    /// Serialize the packet to binary format for WebSocket transmission
    /// Format: [timestamp(8 bytes)][frame_count(4 bytes)][frame1_len(4 bytes)][frame1_data][frame2_len(4 bytes)][frame2_data]...
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(8 + 4);
        data.extend_from_slice(&self.timestamp.to_le_bytes());
        data.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());

        for frame in &self.frames {
            data.extend_from_slice(&(frame.len() as u32).to_le_bytes());
            data.extend_from_slice(frame);
        }

        data
    }

    /// Deserialize a packet from binary format
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }

        let timestamp = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
        let frame_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;

        let mut frames = Vec::with_capacity(frame_count);
        let mut offset = 12;

        for _ in 0..frame_count {
            if offset + 4 > data.len() {
                return None;
            }

            let frame_len = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;

            if offset + frame_len > data.len() {
                return None;
            }

            frames.push(data[offset..offset + frame_len].to_vec());
            offset += frame_len;
        }

        Some(Self {
            timestamp,
            frames,
        })
    }

    /// Get the total number of frames in this packet
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_single_frame() {
        let packet = SpectrogramPacket::new(12345, vec![1, 2, 3, 4, 5]);
        let bytes = packet.to_bytes();
        let restored = SpectrogramPacket::from_bytes(&bytes).unwrap();

        assert_eq!(restored.timestamp, packet.timestamp);
        assert_eq!(restored.frames.len(), 1);
        assert_eq!(restored.frames[0], packet.frames[0]);
    }

    #[test]
    fn test_serialize_deserialize_multiple_frames() {
        let packet = SpectrogramPacket::new_with_frames(12345, vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
        ]);
        let bytes = packet.to_bytes();
        let restored = SpectrogramPacket::from_bytes(&bytes).unwrap();

        assert_eq!(restored.timestamp, packet.timestamp);
        assert_eq!(restored.frames.len(), 3);
        assert_eq!(restored.frames[0], vec![1, 2, 3]);
        assert_eq!(restored.frames[1], vec![4, 5, 6]);
        assert_eq!(restored.frames[2], vec![7, 8, 9]);
    }

    #[test]
    fn test_empty_frames() {
        let packet = SpectrogramPacket::new_with_frames(0, vec![]);
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), 12); // timestamp + frame_count
    }
}