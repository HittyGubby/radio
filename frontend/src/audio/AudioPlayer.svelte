<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { OpusDecoder } from "opus-decoder";

  const WS_URL = "/audio";
  const CONFIG_URL = "/config";

  interface AudioConfig {
    list_devices: boolean;
    device_name: string | null;
    device_id: number | null;
    sample_rate: number;
    channels: number;
    ring_buffer_seconds: number;
    audio_codec: string;
    audio_bitrate: number;
    audio_bitdepth: number;
    audio_frame_ms: number;
    fft_size: number;
    fft_overlap: number;
    spectro_bins: number;
    spectro_fps: number;
    ws_bind: string;
    audio_path: string;
    spectro_path: string;
    max_clients: number;
    jitter_buffer_ms: number;
  }

  let audioConfig: AudioConfig | null = null;
  let SAMPLE_RATE = 48000;
  let USE_OPUS = true;
  let BIT_DEPTH = 16;

  let audioContext: AudioContext | null = null;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let packetCount: number = 0;
  let latency: number = 0;
  let opusDecoder: any = null;
  let workletNode: any = null;
  let feedIntervalId: number | null = null;

  let JITTER_BUFFER_SAMPLES = (SAMPLE_RATE * 100) / 1000;
  let MIN_BUFFER_SAMPLES = (SAMPLE_RATE * 50) / 1000;
  let jitterBuffer: Float32Array = new Float32Array(JITTER_BUFFER_SAMPLES);
  let jitterBufferWritePos = 0;
  let jitterBufferReadPos = 0;
  let jitterBufferFilled = false;
  let playbackStarted = false;
  let samplesFedToWorklet = 0;
  let lastReportedTotalProcessed = 0;
  let opusDecodeErrors = 0;
  let opusDecoderReady = false;

  export let onLatencyUpdate: (latency: number) => void = () => {};

  async function fetchAudioConfig() {
    try {
      const response = await fetch(CONFIG_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const config = (await response.json()) as AudioConfig;
      audioConfig = config;
      SAMPLE_RATE = config.sample_rate;
      USE_OPUS = config.audio_codec === "opus";
      BIT_DEPTH = config.audio_bitdepth;

      JITTER_BUFFER_SAMPLES = (SAMPLE_RATE * config.jitter_buffer_ms) / 1000;
      MIN_BUFFER_SAMPLES = (SAMPLE_RATE * (config.jitter_buffer_ms / 2)) / 1000;
      jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);
    } catch (e) {
      console.error("Failed to fetch audio config:", e);
      audioConfig = {
        sample_rate: 48000,
        channels: 1,
        audio_codec: "opus",
        audio_bitrate: 48000,
        audio_bitdepth: 16,
        audio_frame_ms: 20,
        jitter_buffer_ms: 100,
        list_devices: false,
        device_name: null,
        device_id: null,
        ring_buffer_seconds: 10,
        fft_size: 1024,
        fft_overlap: 0.5,
        spectro_bins: 512,
        spectro_fps: 25,
        ws_bind: "[::]:23331",
        audio_path: "/audio",
        spectro_path: "/spectro",
        max_clients: 200,
      };
      SAMPLE_RATE = audioConfig.sample_rate;
      USE_OPUS = audioConfig.audio_codec === "opus";
      BIT_DEPTH = audioConfig.audio_bitdepth;

      JITTER_BUFFER_SAMPLES = (SAMPLE_RATE * audioConfig.jitter_buffer_ms) / 1000;
      MIN_BUFFER_SAMPLES = (SAMPLE_RATE * (audioConfig.jitter_buffer_ms / 2)) / 1000;
      jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);
    }
  }

  onMount(async () => {
    await fetchAudioConfig();
    initAudioContext();
    if (USE_OPUS) {
      initOpusDecoder();
    }
    connectWebSocket();
  });

  onDestroy(() => {
    if (ws) {
      ws.close();
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
    if (feedIntervalId) {
      clearInterval(feedIntervalId);
    }
    if (audioContext) {
      audioContext.close();
    }
    if (opusDecoder) {
      opusDecoder.free();
    }
  });

  async function initOpusDecoder() {
    try {
      opusDecoder = new OpusDecoder({
        sampleRate: SAMPLE_RATE as any,
        channels: 1,
      });
      await opusDecoder.ready;
      opusDecoderReady = true;
    } catch (e) {
      console.error("Failed to initialize Opus decoder:", e);
      opusDecoder = null;
      opusDecoderReady = false;
    }
  }

  async function initAudioContext() {
    audioContext = new (window.AudioContext || (window as any).webkitAudioContext)({
      sampleRate: SAMPLE_RATE,
    });

    if (audioContext.state === "suspended") {
      await audioContext.resume();
    }

    try {
      await audioContext.audioWorklet.addModule("/src/audio/audio-processor.js");
      workletNode = new AudioWorkletNode(audioContext, "audio-processor", {
        outputChannelCount: [1],
      });

      workletNode.connect(audioContext.destination);

      workletNode.port.onmessage = (event: MessageEvent) => {
        if (event.data.type === "stats") {
          lastReportedTotalProcessed = event.data.totalProcessed;
          latency = parseFloat(event.data.bufferMs);
          onLatencyUpdate(latency);
        }
      };

      startContinuousFeeding();
    } catch (e) {
      console.error("Failed to initialize AudioWorklet:", e);
      initScriptProcessor();
    }
  }

  function initScriptProcessor() {
    let audioBuffer = new Float32Array(0);

    const bufferSize = 4096;
    const scriptProcessor = audioContext!.createScriptProcessor(bufferSize, 0, 1);

    scriptProcessor.onaudioprocess = (event) => {
      const output = event.outputBuffer.getChannelData(0);

      for (let i = 0; i < output.length; i++) {
        if (i < audioBuffer.length) {
          output[i] = audioBuffer[i];
        } else {
          output[i] = 0;
        }
      }

      if (output.length < audioBuffer.length) {
        audioBuffer = audioBuffer.slice(output.length);
      } else {
        audioBuffer = new Float32Array(0);
      }

      latency = (audioBuffer.length / SAMPLE_RATE) * 1000;
      onLatencyUpdate(latency);
    };

    scriptProcessor.connect(audioContext!.destination);
    workletNode = scriptProcessor;

    (workletNode as any).audioBuffer = audioBuffer;

    startContinuousFeeding();
  }

  function startContinuousFeeding() {
    feedIntervalId = window.setInterval(() => {
      feedSamplesToWorklet();
    }, 20);
  }

  function feedSamplesToWorklet() {
    if (!workletNode || !playbackStarted) return;

    const available = getAvailableSamples();

    if (available === 0) {
      return;
    }

    const samplesToFeed = Math.min(960, available);

    if (samplesToFeed > 0) {
      const chunk = new Float32Array(samplesToFeed);

      for (let i = 0; i < samplesToFeed; i++) {
        chunk[i] = jitterBuffer[jitterBufferReadPos];
        jitterBufferReadPos = (jitterBufferReadPos + 1) % JITTER_BUFFER_SAMPLES;
      }

      samplesFedToWorklet += samplesToFeed;

      if (workletNode.port) {
        workletNode.port.postMessage({
          type: "addBuffer",
          samples: chunk,
        });
      } else if (workletNode.audioBuffer) {
        const newBuffer = new Float32Array(workletNode.audioBuffer.length + chunk.length);
        newBuffer.set(workletNode.audioBuffer);
        newBuffer.set(chunk, workletNode.audioBuffer.length);
        workletNode.audioBuffer = newBuffer;
      }
    }
  }

  function getAvailableSamples(): number {
    if (jitterBufferFilled) {
      return (JITTER_BUFFER_SAMPLES + jitterBufferWritePos - jitterBufferReadPos) % JITTER_BUFFER_SAMPLES;
    } else {
      return jitterBufferWritePos - jitterBufferReadPos;
    }
  }

  function connectWebSocket() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const host = window.location.host;
    ws = new WebSocket(`${protocol}//${host}${WS_URL}`);

    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    ws.onmessage = async (event) => {
      const data = new DataView(event.data);
      const timestamp = Number(data.getBigUint64(0, true));
      const payload = new Uint8Array(event.data, 8);

      await queueAudioPacket(payload, timestamp);
      packetCount++;
    };

    ws.onerror = (error) => {
      console.error("Audio WebSocket error:", error);
      scheduleReconnect();
    };

    ws.onclose = () => {
      scheduleReconnect();
    };
  }

  async function queueAudioPacket(payload: Uint8Array, timestamp: number) {
    if (!audioContext) return;

    let samples: Float32Array;

    if (USE_OPUS) {
      if (!opusDecoder || !opusDecoderReady) {
        opusDecoder = null;
        opusDecoderReady = false;
        await initOpusDecoder();
      }

      if (opusDecoder && opusDecoderReady) {
        try {
          if (payload.length === 0) {
            throw new Error("Empty payload");
          }

          const result = opusDecoder.decodeFrame(payload);
          samples = result.channelData[0];
          opusDecodeErrors = 0;
        } catch (e) {
          opusDecodeErrors++;
          console.error(`Opus decode error (${opusDecodeErrors}), payload size: ${payload.length}:`, e);

          if (opusDecodeErrors > 5) {
            console.error("Too many Opus decode errors, disabling Opus and using PCM");
            USE_OPUS = false;
          }

          opusDecoder = null;
          opusDecoderReady = false;

          samples = decodePCM(payload);
        }
      } else {
        samples = decodePCM(payload);
      }
    } else {
      samples = decodePCM(payload);
    }

    addToJitterBuffer(samples);
  }

  function addToJitterBuffer(samples: Float32Array) {
    for (let i = 0; i < samples.length; i++) {
      jitterBuffer[jitterBufferWritePos] = samples[i];
      jitterBufferWritePos = (jitterBufferWritePos + 1) % JITTER_BUFFER_SAMPLES;

      if (jitterBufferWritePos === jitterBufferReadPos && !jitterBufferFilled) {
        jitterBufferFilled = true;
      }
    }

    if (!playbackStarted) {
      const available = getAvailableSamples();
      if (available >= MIN_BUFFER_SAMPLES) {
        playbackStarted = true;
      }
    }
  }

  function decodePCM(payload: Uint8Array): Float32Array {
    let samples: Float32Array;

    if (BIT_DEPTH === 16) {
      const sampleCount = payload.length / 2;
      samples = new Float32Array(sampleCount);

      for (let i = 0; i < sampleCount; i++) {
        const sample = new DataView(payload.buffer, i * 2, 2).getInt16(0, true);
        samples[i] = sample / 32768.0;
      }
    } else if (BIT_DEPTH === 24) {
      const sampleCount = payload.length / 3;
      samples = new Float32Array(sampleCount);

      for (let i = 0; i < sampleCount; i++) {
        const byte1 = payload[i * 3];
        const byte2 = payload[i * 3 + 1];
        const byte3 = payload[i * 3 + 2];
        const sample = (byte1 | (byte2 << 8) | (byte3 << 16));
        const signed = sample > 8388607 ? sample - 16777216 : sample;
        samples[i] = signed / 8388608.0;
      }
    } else {
      throw new Error(`Unsupported bit depth: ${BIT_DEPTH}`);
    }

    return samples;
  }

  function scheduleReconnect() {
    if (reconnectTimer) return;

    if (feedIntervalId) {
      clearInterval(feedIntervalId);
      feedIntervalId = null;
    }

    jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);
    jitterBufferWritePos = 0;
    jitterBufferReadPos = 0;
    jitterBufferFilled = false;
    playbackStarted = false;
    samplesFedToWorklet = 0;
    lastReportedTotalProcessed = 0;

    if (workletNode && workletNode.port) {
      workletNode.port.postMessage({ type: "clear" });
    }

    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      if (audioContext && audioContext.state === "suspended") {
        audioContext.resume();
      }
      connectWebSocket();
    }, 1000);
  }

  export function getPacketsPerSecond(): number {
    return packetCount;
  }

  export function resetPacketCount() {
    packetCount = 0;
  }

  export async function resumeAudio() {
    if (audioContext && audioContext.state === "suspended") {
      await audioContext.resume();
    }
  }
</script>

<svelte:head>
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
</svelte:head>
