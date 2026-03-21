use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrogramPacket {
    /// Multiple frames with individual timestamps
    pub frames: Vec<(u64, Vec<u8>)>,
}

impl SpectrogramPacket {
    /// Create a new spectrogram packet with the given frames
    pub fn new(frames: Vec<(u64, Vec<u8>)>) -> Self {
        Self { frames }
    }

    /// Check if all frames in this packet are empty (zero length data or all zeros)
    pub fn is_empty(&self) -> bool {
        self.frames.iter().all(|(_, frame_data)| {
            // Empty frames (zero length) or frames with all zero values
            frame_data.is_empty() || frame_data.iter().all(|&v| v == 0)
        })
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
}
