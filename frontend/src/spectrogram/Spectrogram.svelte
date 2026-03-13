<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  const SPECTRO_BINS = 512;
  const WS_URL = "/spectro";

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let colorPalette: number[][] = [];

  onMount(() => {
    canvas = document.getElementById("spectro-canvas") as HTMLCanvasElement;
    ctx = canvas.getContext("2d")!;

    resizeCanvas();
    generateColorPalette();
    connectWebSocket();

    window.addEventListener("resize", resizeCanvas);
  });

  onDestroy(() => {
    if (ws) {
      ws.close();
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
    window.removeEventListener("resize", resizeCanvas);
  });

  function resizeCanvas() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }

  function generateColorPalette() {
    for (let i = 0; i < 256; i++) {
      const intensity = i / 255;
      const r = Math.floor(intensity * 255);
      const g = Math.floor(intensity * 150);
      const b = Math.floor(intensity * 50);
      colorPalette.push([r, g, b]);
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
      console.log("Spectrogram WebSocket connected");
    };

    ws.onmessage = (event) => {
      const data = new DataView(event.data);
      const startTimestamp = data.getBigUint64(0, true);
      const rowCount = data.getUint8(8);

      if (rowCount === 0) {
        return;
      }

      let offset = 9;

      const columnData = new Uint8Array(SPECTRO_BINS);

      for (let row = 0; row < rowCount; row++) {
        for (let col = 0; col < SPECTRO_BINS; col++) {
          const intensity = data.getUint8(offset + row * SPECTRO_BINS + col);
          columnData[col] += intensity;
        }
      }

      for (let col = 0; col < SPECTRO_BINS; col++) {
        columnData[col] = Math.floor(columnData[col] / rowCount);
      }

      shiftCanvas(1);

      for (let row = 0; row < canvas.height; row++) {
        const binIndex = Math.floor((row / canvas.height) * SPECTRO_BINS);
        const intensity = columnData[binIndex];
        const [r, g, b] = colorPalette[intensity];

        ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
        ctx.fillRect(canvas.width - 1, row, 1, 1);
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

  function shiftCanvas(shiftAmount: number) {
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
    ctx.putImageData(imageData, -shiftAmount, 0);

    ctx.fillStyle = "#000";
    ctx.fillRect(canvas.width - shiftAmount, 0, shiftAmount, canvas.height);
  }

  function scheduleReconnect() {
    if (reconnectTimer) return;

    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connectWebSocket();
    }, 1000);
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
    z-index: -1;
  }
</style>
