<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  export let analyserNode: AnalyserNode | null = null;

  // ===== TUNABLE PARAMETERS =====
  // Column width in pixels (lower = higher resolution)
  const COLUMN_WIDTH = 1;
  // Scroll speed in pixels per frame (higher = faster scrolling, independent of resolution)
  const SCROLL_SPEED = 3;
  // FFT size for frequency analysis (higher = more frequency resolution)
  const FFT_SIZE = 1024;
  // =================================

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let animationId: number | null = null;
  let spectrogramWorker: Worker | null = null;
  let workerReady = false;
  let pendingImageData: ImageData | null = null;
  let sampleRate: number = 8000;
  let latestColumn: ImageData | null = null;

  onMount(() => {
    canvas = document.getElementById("spectro-canvas") as HTMLCanvasElement;
    ctx = canvas.getContext("2d")!;

    resizeCanvas();
    initWorker();
    startSpectrogram();

    window.addEventListener("resize", resizeCanvas);
  });

  onDestroy(() => {
    if (animationId) {
      cancelAnimationFrame(animationId);
    }
    window.removeEventListener("resize", resizeCanvas);
    if (spectrogramWorker) {
      spectrogramWorker.terminate();
    }
  });

  function resizeCanvas() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Reinitialize latestColumn with new dimensions
    latestColumn = new ImageData(COLUMN_WIDTH, canvas.height);
    latestColumn.data.fill(0);
  }

  function initWorker() {
    spectrogramWorker = new Worker(new URL("../workers/spectrogram-worker.js", import.meta.url));

    spectrogramWorker.onmessage = (e: MessageEvent) => {
      const { type, data } = e.data;

      if (type === "ready") {
        workerReady = true;
      } else if (type === "result") {
        const imageData = new ImageData(data.data, data.width, data.height);
        pendingImageData = imageData;
      }
    };

    spectrogramWorker.postMessage({ type: "init" });
  }

  function startSpectrogram() {
    if (analyserNode) {
      analyserNode.fftSize = FFT_SIZE;
      sampleRate = analyserNode.context.sampleRate;
      analyserNode.smoothingTimeConstant = 0.8;
      analyserNode.minDecibels = -120;
      analyserNode.maxDecibels = 0;
    }

    // Calculate max frequency bins to process
    const maxBins = analyserNode?.frequencyBinCount;

    // Initialize with a blank black column
    latestColumn = new ImageData(COLUMN_WIDTH, canvas.height);
    latestColumn.data.fill(0);

    const poll = () => {
      if (analyserNode && workerReady) {
        const frequencyData = new Uint8Array(analyserNode.frequencyBinCount);
        analyserNode.getByteFrequencyData(frequencyData);

        // Only send frequency data up to MAX_FREQUENCY
        const frequencyDataLimited = frequencyData.slice(0, maxBins);

        spectrogramWorker.postMessage({
          type: "process",
          data: {
            frequencyData: Array.from(frequencyDataLimited),
            maxBins: maxBins,
            canvasWidth: canvas.width,
            canvasHeight: canvas.height,
            columnWidth: COLUMN_WIDTH,
          },
        });
      }

      animationId = requestAnimationFrame(poll);
    };

    const render = (timestamp: number) => {
      // Update latest column when worker sends new data
      if (pendingImageData) {
        latestColumn = pendingImageData;
        pendingImageData = null;
      }

      // Shift existing spectrogram data to the left
      const imageData = ctx.getImageData(SCROLL_SPEED, 0, canvas.width - SCROLL_SPEED, canvas.height);
      ctx.putImageData(imageData, 0, 0);

      // Fill the gap with the latest column (stretch/replicate it)
      if (latestColumn) {
        // Create a temporary canvas to scale the column to fill the gap
        const tempCanvas = document.createElement("canvas");
        tempCanvas.width = COLUMN_WIDTH;
        tempCanvas.height = canvas.height;
        const tempCtx = tempCanvas.getContext("2d")!;
        tempCtx.putImageData(latestColumn, 0, 0);

        // Draw the scaled column to fill the gap
        ctx.drawImage(
          tempCanvas,
          0, // Source X
          0, // Source Y
          COLUMN_WIDTH, // Source width
          canvas.height, // Source height
          canvas.width - SCROLL_SPEED, // Dest X
          0, // Dest Y
          SCROLL_SPEED, // Dest width (fill the gap)
          canvas.height, // Dest height
        );
      }

      animationId = requestAnimationFrame(render);
    };

    animationId = requestAnimationFrame(poll);
    animationId = requestAnimationFrame(render);
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
    transform: scaleY(-100%);
  }
</style>
