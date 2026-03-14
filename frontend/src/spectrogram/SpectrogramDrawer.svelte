<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  // ===== TUNABLE PARAMETERS =====
  // Maximum frequency to display in Hz (will be updated from config)
  let MAX_FREQUENCY = 0;
  // Whether to show frequency grid lines
  const SHOW_GRID = false;
  // Grid line frequencies in Hz
  const GRID_FREQUENCIES: number[] = [100, 500, 1000, 2000, 5000, 10000, 15000];
  // Maximum buffer size (in frames) to prevent memory issues
  const MAX_BUFFER_FRAMES = 100;
  // Smoothing factor for temporal interpolation (0-1, higher = more smoothing)
  const SMOOTHING_FACTOR = 0.5;
  // Column width in pixels (stretch factor) - higher = faster flow
  const COLUMN_WIDTH = 2;
  // =================================

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let animationId: number | null = null;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;

  // FIFO buffer to store frames in order
  let frameQueue: Uint8Array[] = [];
  let previousFrame: Uint8Array | null = null;

  // Offscreen canvas for double buffering
  let offscreenCanvas: HTMLCanvasElement;
  let offscreenCtx: CanvasRenderingContext2D;

  // Smooth rendering system
  let lastDrawTime = 0;
  const TARGET_FPS = 60;
  const FRAME_TIME_MS = 1000 / TARGET_FPS;

  const SPECTRO_URL = "/spectro";
  const CONFIG_URL = "/config";

  onMount(() => {
    canvas = document.getElementById("spectro-canvas") as HTMLCanvasElement;
    ctx = canvas.getContext("2d")!;

    resizeCanvas();
    fetchSpectroConfig();
    connectWebSocket();
    startRendering();

    window.addEventListener("resize", resizeCanvas);
  });

  onDestroy(() => {
    if (animationId) {
      cancelAnimationFrame(animationId);
    }
    window.removeEventListener("resize", resizeCanvas);
    if (ws) {
      ws.close();
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
  });

  function resizeCanvas() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Create offscreen canvas for double buffering
    offscreenCanvas = document.createElement("canvas");
    offscreenCanvas.width = canvas.width;
    offscreenCanvas.height = canvas.height;
    offscreenCtx = offscreenCanvas.getContext("2d")!;
    offscreenCtx.fillStyle = "#000";
    offscreenCtx.fillRect(0, 0, offscreenCanvas.width, offscreenCanvas.height);
  }

  async function fetchSpectroConfig() {
    try {
      const response = await fetch(CONFIG_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const config = await response.json();
      if (config.spectro_max_freq) {
        MAX_FREQUENCY = config.spectro_max_freq;
        console.log("Spectrogram max frequency updated to:", MAX_FREQUENCY);
      }
    } catch (e) {
      console.error("Failed to fetch spectrogram config:", e);
    }
  }

  function connectWebSocket() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const host = window.location.host;
    ws = new WebSocket(`${protocol}//${host}${SPECTRO_URL}`);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
      console.log("Spectrogram WebSocket connected");
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    ws.onmessage = (event) => {
      const data = new DataView(event.data);
      const timestamp = Number(data.getBigUint64(0, true));
      const frameCount = data.getUint32(8, true);

      let offset = 12;

      for (let i = 0; i < frameCount; i++) {
        const frameLen = data.getUint32(offset, true);
        offset += 4;
        const frameData = new Uint8Array(event.data, offset, frameLen);

        // Add frame to queue (FIFO)
        frameQueue.push(frameData);

        offset += frameLen;
      }

      // Prune old frames to prevent memory issues
      while (frameQueue.length > MAX_BUFFER_FRAMES) {
        frameQueue.shift();
      }
    };

    ws.onerror = (error) => {
      console.error("Spectrogram WebSocket error:", error);
      scheduleReconnect();
    };

    ws.onclose = () => {
      scheduleReconnect();
    };
  }

  function scheduleReconnect() {
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connectWebSocket();
    }, 3000);
  }

  function startRendering() {
    lastDrawTime = performance.now();
    requestAnimationFrame(render);
  }

  function drawSmoothFrame() {
    if (frameQueue.length === 0) return;

    // Draw one frame from the queue
    const frame = frameQueue.shift()!;

    const maxBins = frame.length || 0;

    // Shift offscreen canvas to the left by one column width
    offscreenCtx.drawImage(offscreenCanvas, COLUMN_WIDTH, 0, offscreenCanvas.width - COLUMN_WIDTH, offscreenCanvas.height, 0, 0, offscreenCanvas.width - COLUMN_WIDTH, offscreenCanvas.height);

    // Draw the new frame at the right edge
    const x = offscreenCanvas.width - COLUMN_WIDTH;
    drawColumnToOffscreen(frame, previousFrame, x, COLUMN_WIDTH, maxBins);
    previousFrame = frame;
  }

  function render(timestamp: number) {
    // Calculate elapsed time since last draw
    const elapsed = timestamp - lastDrawTime;

    // Draw frames at constant rate
    if (elapsed >= FRAME_TIME_MS) {
      lastDrawTime = timestamp - (elapsed % FRAME_TIME_MS);

      // Draw one frame from queue (or multiple if needed to catch up)
      drawSmoothFrame();
    }

    // Render offscreen canvas to main canvas (smooth 60fps)
    ctx.drawImage(offscreenCanvas, 0, 0);

    // Draw grid on top
    if (SHOW_GRID) {
      drawGrid();
    }

    animationId = requestAnimationFrame(render);
  }

  function addFramesToOffscreen() {
    if (frameQueue.length === 0) return;

    const maxBins = frameQueue[0]?.length || 0;
    const totalWidth = frameQueue.length * COLUMN_WIDTH;

    if (totalWidth > 0) {
      // Shift offscreen canvas to the left
      offscreenCtx.drawImage(offscreenCanvas, totalWidth, 0, offscreenCanvas.width - totalWidth, offscreenCanvas.height, 0, 0, offscreenCanvas.width - totalWidth, offscreenCanvas.height);

      // Draw all frames at the right edge of offscreen canvas
      for (let i = 0; i < frameQueue.length; i++) {
        const currentFrame = frameQueue[i];
        const x = offscreenCanvas.width - totalWidth + i * COLUMN_WIDTH;
        drawColumnToOffscreen(currentFrame, previousFrame, x, COLUMN_WIDTH, maxBins);
        previousFrame = currentFrame;
      }

      // Clear the queue after drawing all frames
      frameQueue = [];
    }
  }

  function drawColumnToOffscreen(frequencyData: Uint8Array, prevFrame: Uint8Array | null, x: number, width: number, maxBins: number) {
    // Create ImageData for the column with specified width
    const imageData = offscreenCtx.createImageData(width, offscreenCanvas.height);
    const data = imageData.data;

    for (let y = 0; y < offscreenCanvas.height; y++) {
      // Use logarithmic scaling for better frequency distribution
      const normalizedY = y / offscreenCanvas.height;
      const exactBinIndex = Math.pow(normalizedY, 1.5) * maxBins;

      // Get the two nearest bins for interpolation
      const binIndexLow = Math.floor(exactBinIndex);
      const binIndexHigh = Math.min(binIndexLow + 1, maxBins - 1);
      const interpolationFactor = exactBinIndex - binIndexLow;

      // Get intensities from current frame
      let intensityLow = binIndexLow < frequencyData.length ? frequencyData[binIndexLow] : 0;
      let intensityHigh = binIndexHigh < frequencyData.length ? frequencyData[binIndexHigh] : 0;

      // Interpolate between adjacent bins in frequency domain
      let intensity = Math.floor(intensityLow * (1 - interpolationFactor) + intensityHigh * interpolationFactor);

      // Apply temporal smoothing with previous frame
      if (prevFrame) {
        let prevIntensityLow = binIndexLow < prevFrame.length ? prevFrame[binIndexLow] : 0;
        let prevIntensityHigh = binIndexHigh < prevFrame.length ? prevFrame[binIndexHigh] : 0;

        // Interpolate between adjacent bins in previous frame
        let prevIntensity = Math.floor(prevIntensityLow * (1 - interpolationFactor) + prevIntensityHigh * interpolationFactor);

        // Temporal smoothing
        intensity = Math.floor(intensity * (1 - SMOOTHING_FACTOR) + prevIntensity * SMOOTHING_FACTOR);
      }

      const [r, g, b] = getPaletteColor(intensity);

      // Fill all pixels in the column width
      for (let px = 0; px < width; px++) {
        const pixelIndex = (y * width + px) * 4;
        data[pixelIndex] = r;
        data[pixelIndex + 1] = g;
        data[pixelIndex + 2] = b;
        data[pixelIndex + 3] = 255;
      }
    }

    offscreenCtx.putImageData(imageData, x, 0);
  }

  function getPaletteColor(intensity: number): [number, number, number] {
    const i = intensity / 255;
    const a = 2;
    const x = -Math.log(1 - i) / Math.log(a);
    return [Math.round(x * 20), Math.round(x * 20), Math.round(x * 40)];
  }

  function drawGrid() {
    ctx.strokeStyle = "rgba(255, 255, 255, 0.15)";
    ctx.lineWidth = 1;
    ctx.font = "10px monospace";
    ctx.fillStyle = "rgba(255, 255, 255, 0.5)";

    for (const freq of GRID_FREQUENCIES) {
      if (freq >= MAX_FREQUENCY) continue;

      // Calculate y position based on logarithmic scale
      const normalizedFreq = freq / MAX_FREQUENCY;
      const y = Math.pow(normalizedFreq, 1.5) * canvas.height;

      // Draw line
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();

      // Draw label
      const label = freq >= 1000 ? `${freq / 1000}k` : `${freq}`;
      ctx.fillText(label, 5, y - 3);
    }
  }
</script>

<canvas id="spectro-canvas"></canvas>

<style>
  canvas {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    z-index: 1;
    transform: scaleY(-1);
  }
</style>
