<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { OpusDecoder } from "opus-decoder";
  import { createEventDispatcher } from "svelte";

  const WS_URL = "/audio";
  const CONFIG_URL = "/config";
  const dispatch = createEventDispatcher();

  interface AudioConfig {
    sample_rate: number;
    audio_bitrate: number;
    jitter_buffer_ms: number;
  }

  let audioConfig: AudioConfig | null = null;
  let SAMPLE_RATE = 48000;

  let audioContext: AudioContext | null = null;
  let analyserNode: AnalyserNode | null = null;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let packetTimestamps: number[] = [];
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

  async function fetchAudioConfig() {
    try {
      const response = await fetch(CONFIG_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const config = (await response.json()) as AudioConfig;
      audioConfig = config;
      SAMPLE_RATE = config.sample_rate;
      JITTER_BUFFER_SAMPLES = (SAMPLE_RATE * config.jitter_buffer_ms) / 1000;
      MIN_BUFFER_SAMPLES = (SAMPLE_RATE * (config.jitter_buffer_ms / 2)) / 1000;
      jitterBuffer = new Float32Array(JITTER_BUFFER_SAMPLES);
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

    analyserNode = audioContext.createAnalyser();
    analyserNode.fftSize = 1024;
    analyserNode.smoothingTimeConstant = 0.0;
    analyserNode.minDecibels = -100;
    analyserNode.maxDecibels = 0;

    if (audioContext.state === "suspended") {
      await audioContext.resume();
    }

    try {
      if (audioContext.audioWorklet) {
        await audioContext.audioWorklet.addModule(new URL("../workers/audio-processor.js", import.meta.url));

        workletNode = new AudioWorkletNode(audioContext, "audio-processor", {
          outputChannelCount: [1],
        });

        workletNode.connect(analyserNode);
        analyserNode.connect(audioContext.destination);

        workletNode.port.onmessage = (event: MessageEvent) => {
          if (event.data.type === "stats") {
            lastReportedTotalProcessed = event.data.totalProcessed;
          }
        };

        dispatch("audioContextReady", audioContext);
        startContinuousFeeding();
      } else {
        initScriptProcessor();
      }
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
    };

    scriptProcessor.connect(analyserNode!);
    analyserNode!.connect(audioContext!.destination);
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
    const wsUrl = `${protocol}//${host}${WS_URL}`;
    ws = new WebSocket(wsUrl);
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
      packetTimestamps.push(Date.now());

      // Keep only timestamps from the last 5 seconds (sliding window)
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
        opusDecodeErrors = 0;
      } catch (e) {
        opusDecodeErrors++;
        console.error(`Opus decode error (${opusDecodeErrors}), payload size: ${payload.length}:`, e);

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
    lastReportedTotalProcessed = 0;
    packetTimestamps = [];

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

  export function getPacketPerSec(): number {
    if (packetTimestamps.length === 0) return 0;
    const now = Date.now();
    const oldestTimestamp = packetTimestamps[0];
    const timeWindowSeconds = (now - oldestTimestamp) / 1000;
    if (timeWindowSeconds === 0) return packetTimestamps.length;
    return (packetTimestamps.length / timeWindowSeconds).toFixed(2) as unknown as number;
  }

  export function getAnalyserNode(): AnalyserNode | null {
    return analyserNode;
  }

  export async function resumeAudio() {
    if (audioContext && audioContext.state === "suspended") {
      await audioContext.resume();
    }
  }

  export function getAudioStats() {
    return {
      sampleRate: SAMPLE_RATE,
      bitrate: audioConfig?.audio_bitrate || 48000,
      jitterBufferSamples: getAvailableSamples(),
      jitterBufferCapacity: JITTER_BUFFER_SAMPLES,
      wsConnected: ws !== null && ws.readyState === WebSocket.OPEN,
      playbackStarted: playbackStarted,
    };
  }
</script>
