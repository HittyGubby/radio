<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { OpusDecoder } from "opus-decoder";
  import { createEventDispatcher } from "svelte";

  const WS_URL = "/audio";
  const CONFIG_URL = "/config";
  const dispatch = createEventDispatcher();

  interface AudioConfig {
    output_sample_rate: number;
    audio_bitrate: number;
    jitter_buffer_ms: number;
    input_sample_rate: number;
  }

  let audioConfig: AudioConfig | null = null;
  let OUTPUT_SAMPLE_RATE = 0;

  let audioContext: AudioContext | null = null;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let packetTimestamps: number[] = [];
  let opusDecoder: any = null;
  let workletNode: any = null;
  let feedIntervalId: number | null = null;

  let JITTER_BUFFER_SAMPLES = 0;
  let MIN_BUFFER_SAMPLES = 0;
  let jitterBuffer: Float32Array = new Float32Array(0);
  let jitterBufferWritePos = 0;
  let jitterBufferReadPos = 0;
  let jitterBufferFilled = false;
  let playbackStarted = false;
  let samplesFedToWorklet = 0;
  let opusDecoderReady = false;

  let currentAudioTimestamp = 0;

  async function fetchAudioConfig() {
    try {
      const response = await fetch(CONFIG_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const config = (await response.json()) as AudioConfig;
      audioConfig = config;
      OUTPUT_SAMPLE_RATE = config.output_sample_rate;
      JITTER_BUFFER_SAMPLES = Math.floor((OUTPUT_SAMPLE_RATE * config.jitter_buffer_ms) / 1000);
      MIN_BUFFER_SAMPLES = Math.floor(JITTER_BUFFER_SAMPLES / 2);
      jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);
      console.log("Audio config loaded:", OUTPUT_SAMPLE_RATE);
    } catch (e) {
      console.error("Failed to fetch audio config:", e);
    }
  }

  onMount(async () => {
    await fetchAudioConfig();
    initAudioContext();
    initOpusDecoder();
    connectWebSocket();
  });

  onDestroy(() => {
    if (ws) ws.close();
    if (reconnectTimer) clearTimeout(reconnectTimer);
    if (feedIntervalId) clearInterval(feedIntervalId);
    if (audioContext) audioContext.close();
    if (opusDecoder) opusDecoder.free();
  });

  async function initOpusDecoder() {
    try {
      opusDecoder = new OpusDecoder({
        sampleRate: OUTPUT_SAMPLE_RATE as any,
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
    if (OUTPUT_SAMPLE_RATE === 0) {
      console.error("Output sample rate not set");
      return;
    }

    const contextOptions: AudioContextOptions = {};
    contextOptions.sampleRate = OUTPUT_SAMPLE_RATE;

    audioContext = new (window.AudioContext || (window as any).webkitAudioContext)(contextOptions);

    const jitterBufferMs = audioConfig?.jitter_buffer_ms || 200;
    JITTER_BUFFER_SAMPLES = Math.floor((OUTPUT_SAMPLE_RATE * jitterBufferMs) / 1000);
    MIN_BUFFER_SAMPLES = Math.floor(JITTER_BUFFER_SAMPLES / 2);
    jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);

    resumeAudio();

    if (!audioContext.audioWorklet) {
      initScriptProcessor();
    } else {
      try {
        await audioContext.audioWorklet.addModule(new URL("../workers/audio-processor.js", import.meta.url));
        workletNode = new AudioWorkletNode(audioContext, "audio-processor", {
          outputChannelCount: [1],
        });

        workletNode.connect(audioContext.destination);
        dispatch("audioContextReady", audioContext);
        startContinuousFeeding();
      } catch (e) {
        console.error("Failed to initialize AudioWorklet:", e);
        initScriptProcessor();
      }
    }
  }

  function initScriptProcessor() {
    let audioBuffer = new Float32Array(0);
    const bufferSize = 4096;
    const scriptProcessor = audioContext!.createScriptProcessor(bufferSize, 0, 1);

    scriptProcessor.onaudioprocess = (event) => {
      const output = event.outputBuffer.getChannelData(0);

      for (let i = 0; i < output.length; i++) {
        output[i] = i < audioBuffer.length ? audioBuffer[i] : 0;
      }

      audioBuffer = output.length < audioBuffer.length ? audioBuffer.slice(output.length) : new Float32Array(0);
    };

    scriptProcessor.connect(audioContext!.destination);
    workletNode = scriptProcessor;
    (workletNode as any).audioBuffer = audioBuffer;
    dispatch("audioContextReady", audioContext);
    startContinuousFeeding();
  }

  function startContinuousFeeding() {
    feedIntervalId = window.setInterval(() => {
      feedSamplesToWorklet();
    }, 20);
  }

  function feedSamplesToWorklet() {
    if (!workletNode || !playbackStarted) return;
    const samplesToFeed = getAvailableSamples();
    if (samplesToFeed === 0) return;
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
      const maxBufferSize = OUTPUT_SAMPLE_RATE * 2; // Max 2 seconds of buffered audio
      const currentLength = workletNode.audioBuffer.length;
      const newLength = currentLength + chunk.length;

      if (newLength > maxBufferSize) {
        const keepLength = Math.floor(maxBufferSize * 0.75);
        const droppedSamples = currentLength - keepLength;
        const newBuffer = new Float32Array(keepLength + chunk.length);
        newBuffer.set(workletNode.audioBuffer.subarray(droppedSamples));
        newBuffer.set(chunk, keepLength);
        workletNode.audioBuffer = newBuffer;
      } else {
        const newBuffer = new Float32Array(newLength);
        newBuffer.set(workletNode.audioBuffer);
        newBuffer.set(chunk, currentLength);
        workletNode.audioBuffer = newBuffer;
      }
    }
  }

  function getAvailableSamples(): number {
    if (jitterBufferFilled) {
      return (JITTER_BUFFER_SAMPLES + jitterBufferWritePos - jitterBufferReadPos) % JITTER_BUFFER_SAMPLES;
    }
    const available = jitterBufferWritePos - jitterBufferReadPos;
    return Math.max(0, available);
  }

  function connectWebSocket() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const host = window.location.host;
    const wsUrl = `${protocol}//${host}${WS_URL}`;
    ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";
    ws.onopen = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      resumeAudio();
    };

    ws.onmessage = async (event) => {
      const data = new DataView(event.data);
      const timestamp = Number(data.getBigUint64(0, true));
      const payload = new Uint8Array(event.data, 8);

      currentAudioTimestamp = timestamp;
      await queueAudioPacket(payload, timestamp);
      packetTimestamps.push(Date.now());

      const now = Date.now();
      packetTimestamps = packetTimestamps.filter((ts) => now - ts < 1000);
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

    let samples: Float32Array = new Float32Array(0);

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
      } catch (e) {
        console.error("Opus decode error:", e);
        opusDecoder = null;
        opusDecoderReady = false;
      }
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
    packetTimestamps = [];

    if (workletNode && workletNode.port) {
      workletNode.port.postMessage({ type: "clear" });
    }

    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      resumeAudio();
      connectWebSocket();
    }, 1000);
  }

  export function getPacketPerSec(): number {
    if (packetTimestamps.length === 0) return 0;
    const now = Date.now();
    const oldestTimestamp = packetTimestamps[0];
    const timeWindowSeconds = (now - oldestTimestamp) / 1000;
    if (timeWindowSeconds === 0) return packetTimestamps.length;
    return (packetTimestamps.length / timeWindowSeconds).toFixed(2) as unknown as number;
  }

  export function getCurrentAudioTimestamp(): number {
    return currentAudioTimestamp;
  }

  export async function resumeAudio() {
    if (audioContext && audioContext.state === "suspended") {
      await audioContext.resume();
    }
  }

  export function getAudioStats() {
    return {
      sampleRate: OUTPUT_SAMPLE_RATE,
      bitrate: audioConfig?.audio_bitrate,
      jitterBufferSamples: getAvailableSamples(),
      jitterBufferCapacity: JITTER_BUFFER_SAMPLES,
      wsConnected: ws !== null && ws.readyState === WebSocket.OPEN,
      playbackStarted: playbackStarted,
    };
  }
</script>
