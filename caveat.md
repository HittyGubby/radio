
---

# 1. Encode Once, Broadcast Many

Never encode per-client.

Pipeline must be:

```
capture → ring buffer → encoder → broadcast packets
```

Encoded packets are reused for all clients.

CPU stays constant regardless of listeners.
This is how systems like Icecast scale.

---

# 2. Separate Real-Time Workers

Audio capture must never share a loop with heavy work.

Minimum threads/tasks:

```
audio capture
audio encoder
FFT worker
network broadcast
```

Audio capture should only push samples to the ring buffer.

---

# 3. Lock-Free Ring Buffer

Single producer, multiple consumers.

Rules:

```
producer never blocks
consumers can skip ahead if lagging
```

Size guideline:

```
sample_rate × 10 seconds
```

---

# 4. Fixed Audio Frame Size

Always encode constant frames.

Typical:

```
20 ms
```

Example:

```
48000 Hz → 960 samples
```

This keeps jitter buffering predictable.

---

# 5. Preallocate All Buffers

No memory allocation inside processing loops.

Allocate once:

```
audio frame buffers
FFT buffers
spectrogram rows
network packets
```

Reuse them forever.

---

# 6. Precompute Window Functions

FFT window coefficients must be calculated once.

Example:

```
hann_window[i]
```

Reuse for every frame.

---

# 7. Overlapping FFT Windows

Without overlap waterfalls flicker.

Typical overlap:

```
50–75%
```

Example:

```
1024 FFT
512 step
```

---

# 8. FFT Downsampling

Display bins < FFT bins.

Example:

```
FFT bins: 1024
display bins: 512
```

Combine adjacent bins.

Reduces bandwidth and drawing cost.

---

# 9. Log Frequency Mapping

Human hearing is logarithmic.

Map frequency bins with log scaling before display.

Linear waterfalls look bad for music.

---

# 10. Dynamic Range Compression

Convert magnitude to dB.

Clamp range:

```
-90 dB → black
-20 dB → bright
```

This prevents the waterfall from being mostly dark.

---

# 11. Temporal Smoothing

Spectrogram rows should be smoothed.

Formula:

```
new = α * current + (1-α) * previous
α ≈ 0.4
```

Removes flicker.

---

# 12. Batch Spectrogram Rows

Don’t send rows individually.

Instead:

```
5 rows per packet
```

Reduces WebSocket overhead dramatically.

---

# 13. Binary Network Protocol

Never send spectrogram or audio in JSON.

Binary layout example:

```
timestamp (8 bytes)
row_count (1 byte)
row_data
```

Bandwidth drops and parsing becomes trivial.

---

# 14. Encoded Frame History

Keep a small history buffer.

Example:

```
2 seconds of audio frames
```

When a client connects:

```
send history
then live packets
```

This fills jitter buffers instantly.

---

# 15. Spectrogram History

Also keep recent spectrogram rows.

Example:

```
3 seconds
```

New clients see an already populated waterfall.

---

# 16. Drop Slow Clients

Never let a slow client stall the server.

Policy:

```
if send queue full → drop packets
```

Streaming servers always prefer continuity over reliability.

---

# 17. Timestamp Everything Using Sample Index

Never use wall-clock time.

Use:

```
timestamp = sample_index
```

Convert to time when needed.

This guarantees sync between audio and spectrogram.

---

# 18. Client Jitter Buffer

Browser must buffer packets.

Typical:

```
150–250 ms
```

Without it playback will crackle.

---

# 19. Audio Scheduling

Audio must be scheduled slightly in the future using the browser audio clock via the WebAudio API.

Example concept:

```
play_time = audioContext.currentTime + buffer_delay
```

Never play immediately on packet arrival.

---

# 20. Drift Correction

Browser clocks drift over long sessions.

Monitor buffer size:

```
buffer < 150ms → increase delay
buffer > 300ms → drop frame
```

Keeps latency stable.

---

# 21. Canvas Shift Rendering

For waterfalls, never redraw the full image.

Instead:

```
shift canvas left
draw new column
```

This reduces rendering cost massively.

---

# 22. Use Typed Arrays in Browser

Spectrogram rows should go directly into:

```
Uint8Array
```

Avoid per-pixel object creation.

---

# 23. Client-Side Color Palette

Server sends grayscale intensity.

Browser maps it to colors.

Much cheaper than sending RGB.

---

# 24. Spectrogram FPS Limit

Spectrogram doesn’t need audio-rate updates.

Typical:

```
20–30 FPS
```

Lower update rate saves CPU.

---

# 25. Lazy WebSocket Workers

Only start encoding/broadcast if at least one client is connected.

If nobody is listening:

```
pause encoder and FFT workers
```

Huge CPU saver for idle servers.

---

# 26. Frontend Connection Recovery

If WebSocket disconnects:

```
retry with exponential backoff
```

Example:

```
1s → 2s → 5s → 10s
```

---

# 27. Vite Proxy for Dev

To avoid CORS and allow HMR, configure Vite dev server to proxy backend endpoints.

Then frontend uses relative URLs.

---

# 28. Logging and Metrics

Expose counters:

```
audio packets/sec
spectrogram rows/sec
client count
buffer latency
```

Use Rust structured logging.

---

# 29. Keep the Backend Stateless

Frontend should hold UI state.

Backend should only stream:

```
audio
spectrogram
```

Everything else comes from the external metadata API.

---

# 30. Minimal Latency Strategy

End-to-end latency target:

```
~300 ms
```

Components:

```
audio frame 20ms
jitter buffer 200ms
network ~50ms
```

This feels real-time.

---

# The Only Trick That Actually Saves ~70% CPU

Don’t compute FFT for every audio frame.

Instead:

```
spectrogram_rate = 25 FPS
```

Example:

```
audio frames = 50/sec
FFT frames = 25/sec
```

Compute FFT only every **other frame**.

The waterfall still looks smooth, but FFT cost halves.

This is what many SDR servers do internally (including systems similar to OpenWebRX).

---