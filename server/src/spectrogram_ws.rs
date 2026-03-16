use serde::{Deserialize, Serialize};

/// Spectrogram data packet sent over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrogramPacket {
    /// Multiple frames with individual timestamps
    pub frames: Vec<(u64, Vec<u8>)>,
}

impl SpectrogramPacket {
    /// Create a new spectrogram packet with a single frame
    pub fn new(timestamp: u64, frequency_data: Vec<u8>) -> Self {
        Self {
            frames: vec![(timestamp, frequency_data)],
        }
    }

    /// Create a new spectrogram packet with multiple frames
    pub fn new_with_frames(frames: Vec<(u64, Vec<u8>)>) -> Self {
        Self { frames }
    }

    /// Serialize the packet to binary format for WebSocket transmission
    /// Format: [frame_count(4 bytes)][frame1_timestamp(8 bytes)][frame1_len(4 bytes)][frame1_data][frame2_timestamp(8 bytes)][frame2_len(4 bytes)][frame2_data]...
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(4 + self.frames.len() * 12);
        data.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());

        for (timestamp, frame_data) in &self.frames {
            data.extend_from_slice(&timestamp.to_le_bytes());
            data.extend_from_slice(&(frame_data.len() as u32).to_le_bytes());
            data.extend_from_slice(frame_data);
        }

        data
    }

    /// Deserialize a packet from binary format
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let frame_count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        let mut frames = Vec::with_capacity(frame_count);
        let mut offset = 4;

        for _ in 0..frame_count {
            if offset + 12 > data.len() {
                return None;
            }

            let timestamp = u64::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
                data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
            ]);
            offset += 8;

            let frame_len = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;

            if offset + frame_len > data.len() {
                return None;
            }

            frames.push((timestamp, data[offset..offset + frame_len].to_vec()));
            offset += frame_len;
        }

        Some(Self { frames })
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

        assert_eq!(restored.frames.len(), 1);
        assert_eq!(restored.frames[0].0, 12345);
        assert_eq!(restored.frames[0].1, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_serialize_deserialize_multiple_frames() {
        let packet = SpectrogramPacket::new_with_frames(vec![
            (1000, vec![1, 2, 3]),
            (2000, vec![4, 5, 6]),
            (3000, vec![7, 8, 9]),
        ]);
        let bytes = packet.to_bytes();
        let restored = SpectrogramPacket::from_bytes(&bytes).unwrap();

        assert_eq!(restored.frames.len(), 3);
        assert_eq!(restored.frames[0].0, 1000);
        assert_eq!(restored.frames[0].1, vec![1, 2, 3]);
        assert_eq!(restored.frames[1].0, 2000);
        assert_eq!(restored.frames[1].1, vec![4, 5, 6]);
        assert_eq!(restored.frames[2].0, 3000);
        assert_eq!(restored.frames[2].1, vec![7, 8, 9]);
    }

    #[test]
    fn test_empty_frames() {
        let packet = SpectrogramPacket::new_with_frames(vec![]);
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), 4); // frame_count only
    }
}