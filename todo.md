# Project Overview

Build a **full-stack personal radio streaming web app**.

The system consists of:

```
radio-back/  (Rust backend server)
radio-front/ (Svelte frontend client)
```

The backend captures live audio from a local audio device, generates a spectrogram waterfall in real time, and streams both audio and spectrogram data to clients over WebSocket.

The frontend displays the stream, song metadata, lyrics, and a real-time spectrogram background.

Audio analysis (FFT) **must be done in the backend**, not in the browser.

---

# Core Architecture

```
Audio Device (PipeWire/Pulse)
          │
          ▼
    audio_input module
          │
          ▼
    shared ring buffer
    (single producer)
          │
 ┌────────┴────────┐
 ▼                 ▼
Opus encoder      FFT spectrogram worker
 ▼                 ▼
WS /audio         WS /spectro
 ▼                 ▼
browser audio     waterfall rendering
```

All components use **one shared timeline based on sample index** to guarantee synchronization.

---

# Backend Requirements (Rust)

Location:

```
radio-back/
```

Use Rust with these crates:

* tokio (async runtime)
* tokio-tungstenite (WebSocket)
* cpal (audio capture)
* rustfft (FFT processing)
* ringbuf (lock-free ring buffer)
* serde / serde_json
* clap (CLI config)
* opus (Opus encoder)

Do not reimplement existing algorithms if crates exist.

---

# Backend Configuration

The backend must accept configuration via CLI arguments.

Define a single configuration struct:

```
input_device: string
sample_rate: 48000
channels: 1
ring_buffer_seconds: 10

audio_codec: opus | pcm
audio_bitrate: 64000
audio_bitdepth: 16
audio_frame_ms: 20

fft_size: 1024
fft_overlap: 0.5
spectro_bins: 512
spectro_fps: 25

ws_bind: ":8080"
audio_path: "/audio"
spectro_path: "/spectro"

max_clients: 200
jitter_buffer_ms: 200
```

Provide sensible defaults.

---

# Audio Capture

Use `cpal` to capture audio from a specified device.

Requirements:

* sample rate must match configuration
* convert audio to **f32 mono**
* push samples into a **lock-free ring buffer**

Ring buffer size:

```
capacity = sample_rate × ring_buffer_seconds
```

Example:

```
48000 × 10 = 480000 samples
```

Use **single producer / multiple consumer** design.

---

# Ring Buffer Behavior

Consumers read independently.

If a consumer falls behind too much:

```
read_ptr = write_ptr - safe_margin
```

Never block the producer.

---

# Audio Encoding

Audio frames must be produced every fixed interval.

Example:

```
20 ms frames
```

Frame size:

```
samples = sample_rate × frame_ms
```

Example:

```
48000 × 0.02 = 960 samples
```

Encode using Opus by default.

Audio packet format:

```
struct AudioPacket {
    timestamp: u64   // sample index
    payload: bytes   // encoded frame
}
```

Send packets via WebSocket as **binary frames**.

---

# Spectrogram Generation

Spectrogram must be computed from the same ring buffer.

Use `rustfft`.

FFT pipeline:

1. read overlapping windows
2. apply window function
3. perform FFT
4. convert magnitude to dB
5. normalize dynamic range
6. compress to 0–255 intensity

---

# DSP Requirements

Implement these processing steps:

Window function:

```
Hann window
```

FFT overlap:

```
50–75%
```

Dynamic range:

```
-90 dB → black
-20 dB → bright
```

Temporal smoothing:

```
display = α * current + (1-α) * previous
α = 0.4
```

Spectrogram rows must be downsampled to:

```
spectro_bins
```

---

# Spectrogram Packet Format

Packets contain **multiple rows** to reduce WebSocket overhead.

Batch size:

```
5 rows per packet
```

Packet structure:

```
struct SpectroPacket {
    start_timestamp: u64
    rows: Vec<[u8; spectro_bins]>
}
```

Each row corresponds to one FFT frame.

---

# WebSocket Server

Run a WebSocket server using tokio.

Endpoints:

```
/audio
/spectro
```

Clients connect independently.

Audio packets and spectrogram packets must be broadcast to all connected clients.

Backpressure policy:

If a client cannot keep up:

```
drop packets
```

Do not block encoder threads.

---

# Time Synchronization

All packets reference the same timestamp:

```
timestamp = sample_index
```

The frontend will align audio playback with spectrogram rows using this timestamp.

---

# Frontend Requirements

Location:

```
radio-front/
```

Framework:

```
Svelte + Vite
```

No heavy UI design yet.

Focus on functionality and debugging.

---

# Metadata APIs

The frontend must interact with an existing external API.

Endpoint:

```
/info
```

Example response:

```
{
 "status":"playing",
 "name":"Crna Gora",
 "singer":"Rodoljub \"Roki\" Vulović",
 "albumName":"Crni Bombarder",
 "picUrl":"...",
 "progress":117.353651,
 "duration":250.989569,
 "playbackRate":1,
 "lyric":"",
 "tlyric":""
}
```

Fields `lyric` and `tlyric` contain **LRC formatted lyrics**.

---

# Metadata Fetch Logic

On page load:

```
fetch /info
```

Use this data to populate UI.

---

# Realtime Player Status

Realtime updates come from SSE endpoint:

```
/status
```

Example events:

```
event: progress
data: 152.961451

event: status
data: "paused"
```

Handle only:

```
progress
status
```

---

# Song Change Detection

If SSE emits events for:

```
name
singer
albumName
```

Then the song has changed.

Immediately re-fetch `/info`.

---

# Failsafe Logic

If SSE disconnects:

```
attempt reconnect
```

Also detect song change by:

```
progress >= duration
or
progress resets near zero
```

If detected:

```
refetch /info
```

Prevent duplicate requests by allowing only **one metadata refresh at a time**.

---

# Frontend Debug UI

Display simple debug elements:

```
song name
album name
singer
album cover
lyrics
progress
player status
```

No complex styling.

---

# Spectrogram Rendering

The frontend connects to:

```
WS /spectro
```

The server sends only **new rows**.

Frontend renders a waterfall:

```
scroll direction: right → left
```

Spectrogram should be drawn using **Canvas**.

It should act as a **background layer** beneath UI.

Frontend must **not perform FFT**.

---

# Audio Playback

Frontend connects to:

```
WS /audio
```

Audio packets must be buffered using a **jitter buffer (~200 ms)**.

Playback must be handled using **WebAudio API**.

Audio and spectrogram must be synchronized using packet timestamps.

---

# Debugging Requirements

Include:

* console logs for WebSocket connections
* packet counters
* reconnect logic
* latency display

Make debugging easy.

---

# Development Mode

Provide a development setup where:

* backend runs locally
* frontend dev server connects to backend
* no production build required during development

Use Vite proxy for API routing.

---

# Project Structure

```
radio-project
│
├── radio-back
│   ├── src
│   │   ├── audio_input.rs
│   │   ├── ring_buffer.rs
│   │   ├── encoder.rs
│   │   ├── spectrogram.rs
│   │   ├── ws_audio.rs
│   │   ├── ws_spectro.rs
│   │   ├── config.rs
│   │   └── main.rs
│
└── radio-front
    ├── src
    │   ├── spectrogram
    │   ├── audio
    │   ├── metadata
    │   └── App.svelte
```

Keep modules small and focused.

Avoid large single files.

---