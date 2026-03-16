<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  let canvas: HTMLCanvasElement;
  let width = 0;
  let height = 0;
  let ws: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let device: GPUDevice;
  let context: GPUCanvasContext;
  let pipeline: GPURenderPipeline;
  let texture: GPUTexture;
  let sampler: GPUSampler;
  let bindGroup: GPUBindGroup;
  let pixelBuffer: Uint8Array;
  let frames: Uint8Array[] = [];
  const SPECTRO_URL = "/spectro";
  let animationId: number;

  // Grid configuration - user-specified grid cell size
  const GRID_CELL_SIZE = 12; // Size of each square grid cell in pixels
  let gridRows = 0;
  let gridCols = 0;
  let binsPerRow = 0; // How many frequency bins to average per grid row

  onMount(async () => {
    canvas = document.getElementById("spectro-canvas") as HTMLCanvasElement;
    const adapter = await navigator.gpu.requestAdapter();
    device = await adapter!.requestDevice();
    context = canvas.getContext("webgpu") as GPUCanvasContext;
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: "premultiplied",
    });
    initPipeline(format);
    resize();
    connect();
    render();
    window.addEventListener("resize", resize);
  });

  onDestroy(() => {
    cancelAnimationFrame(animationId);
    ws?.close();
    if (reconnectTimer) clearTimeout(reconnectTimer);
  });

  function resize() {
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    width = canvas.width;
    height = canvas.height;
    pixelBuffer = new Uint8Array(width * height * 4);
    texture?.destroy();
    texture = device.createTexture({
      size: [width, height],
      format: "rgba8unorm",
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sampler },
        { binding: 1, resource: texture.createView() },
      ],
    });

    // Calculate grid dimensions
    gridRows = Math.floor(height / GRID_CELL_SIZE);
    gridCols = Math.floor(width / GRID_CELL_SIZE);

    // Initialize with grid lines
    drawGridOutlines();
  }

  function initPipeline(format: GPUTextureFormat) {
    const shader = device.createShaderModule({
      code: `
struct VOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f
}
@vertex
fn vs(@builtin(vertex_index) i:u32)->VOut{
var pos=array<vec2f,6>(
vec2f(-1,-1),
vec2f(1,-1),
vec2f(-1,1),
vec2f(-1,1),
vec2f(1,-1),
vec2f(1,1)
);
var o:VOut;
o.pos=vec4f(pos[i],0,1);
o.uv=(pos[i]+1)*0.5;
o.uv.y=1-o.uv.y;
return o;
}
@group(0)@binding(0)var s:sampler;
@group(0)@binding(1)var t:texture_2d<f32>;
@fragment
fn fs(@location(0)uv:vec2f)->@location(0)vec4f{
return textureSample(t,s,uv);
}
`,
    });
    pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module: shader, entryPoint: "vs" },
      fragment: { module: shader, entryPoint: "fs", targets: [{ format }] },
      primitive: { topology: "triangle-list" },
    });
    sampler = device.createSampler({
      magFilter: "nearest",
      minFilter: "nearest",
    });
  }

  function connect() {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    ws = new WebSocket(`${proto}://${location.host}${SPECTRO_URL}`);
    ws.binaryType = "arraybuffer";
    ws.onmessage = (e) => {
      const dv = new DataView(e.data);
      let offset = 0;
      const count = dv.getUint32(offset, true);
      offset += 4;

      for (let i = 0; i < count; i++) {
        const ts = Number(dv.getBigUint64(offset, true));
        offset += 8;
        const len = dv.getUint32(offset, true);
        offset += 4;
        frames.push(new Uint8Array(e.data, offset, len));
        offset += len;
      }

      // Set up bins per row on first frame
      if (binsPerRow === 0 && frames.length > 0 && gridRows > 0) {
        binsPerRow = Math.ceil(frames[0].length / gridRows);
      }

      // Prune old frames
      if (frames.length > 200) {
        frames.splice(0, frames.length - 200);
      }
    };
    ws.onclose = () => {
      reconnectTimer = setTimeout(connect, 3000);
    };
  }

  function render() {
    draw();
    renderGPU();
    animationId = requestAnimationFrame(render);
  }

  function draw() {
    if (frames.length === 0 || gridRows === 0 || binsPerRow === 0) return;

    // Draw ONE frame at a time (incremental grid update)
    const frame = frames.shift()!;
    shiftGrid(1);
    clearNewColumn(gridCols - 1);
    drawGridColumn(frame, gridCols - 1);
    drawGridOutlines();
  }

  function shiftGrid(numCols: number) {
    // Shift entire grid left by numCols cells
    const cellSize = GRID_CELL_SIZE;

    for (let row = 0; row < gridRows; row++) {
      const rowY = row * cellSize;
      for (let col = 0; col < gridCols - numCols; col++) {
        const srcX = (col + numCols) * cellSize;
        const dstX = col * cellSize;

        // Copy the cell interior (leaving 1-pixel gap for grid lines)
        for (let y = 0; y < cellSize - 1; y++) {
          const rowOffset = (rowY + y) * width * 4;
          for (let x = 0; x < cellSize - 1; x++) {
            const srcIdx = rowOffset + (srcX + x) * 4;
            const dstIdx = rowOffset + (dstX + x) * 4;
            pixelBuffer[dstIdx] = pixelBuffer[srcIdx];
            pixelBuffer[dstIdx + 1] = pixelBuffer[srcIdx + 1];
            pixelBuffer[dstIdx + 2] = pixelBuffer[srcIdx + 2];
            pixelBuffer[dstIdx + 3] = 255;
          }
        }
      }
    }
  }

  function clearNewColumn(colIndex: number) {
    const cellSize = GRID_CELL_SIZE;
    const colX = colIndex * cellSize;

    for (let row = 0; row < gridRows; row++) {
      const rowY = row * cellSize;
      for (let y = 0; y < cellSize; y++) {
        const rowOffset = (rowY + y) * width * 4;
        for (let x = 0; x < cellSize; x++) {
          const idx = rowOffset + (colX + x) * 4;
          pixelBuffer[idx] = 0;
          pixelBuffer[idx + 1] = 0;
          pixelBuffer[idx + 2] = 0;
          pixelBuffer[idx + 3] = 255;
        }
      }
    }
  }

  function drawGridColumn(frame: Uint8Array, colIndex: number) {
    const cellSize = GRID_CELL_SIZE;
    const colX = colIndex * cellSize;

    for (let row = 0; row < gridRows; row++) {
      // Average the bins for this row
      let sum = 0;
      let count = 0;

      for (let b = 0; b < binsPerRow; b++) {
        const binIndex = row * binsPerRow + b;
        if (binIndex < frame.length) {
          sum += frame[binIndex];
          count++;
        }
      }

      const value = count > 0 ? Math.floor(sum / count) : 0;
      const [r, g, b] = palette(value);
      const rowY = row * cellSize;

      // Draw the cell, leaving 1-pixel gap for grid lines
      for (let y = 0; y < cellSize - 1; y++) {
        const rowOffset = (rowY + y) * width * 4;
        for (let x = 0; x < cellSize - 1; x++) {
          const idx = rowOffset + (colX + x) * 4;
          pixelBuffer[idx] = r;
          pixelBuffer[idx + 1] = g;
          pixelBuffer[idx + 2] = b;
          pixelBuffer[idx + 3] = 255;
        }
      }
    }
  }

  function renderGPU() {
    device.queue.writeTexture({ texture }, pixelBuffer, { bytesPerRow: width * 4 }, { width, height });
    const enc = device.createCommandEncoder();
    const view = context.getCurrentTexture().createView();
    const pass = enc.beginRenderPass({
      colorAttachments: [
        {
          view,
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(6);
    pass.end();
    device.queue.submit([enc.finish()]);
  }

  function drawGridOutlines() {
    if (gridRows === 0 || gridCols === 0) return;

    const cellSize = GRID_CELL_SIZE;
    const gridWidth = gridCols * cellSize;
    const gridHeight = gridRows * cellSize;

    // Draw horizontal lines (at bottom of each row)
    for (let row = 0; row <= gridRows; row++) {
      const y = row * cellSize;
      for (let x = 0; x < gridWidth; x++) {
        const idx = (y * width + x) * 4;
        pixelBuffer[idx] = 30;
        pixelBuffer[idx + 1] = 30;
        pixelBuffer[idx + 2] = 30;
        pixelBuffer[idx + 3] = 255;
      }
    }

    // Draw vertical lines (at right of each column)
    for (let col = 0; col <= gridCols; col++) {
      const x = col * cellSize;
      for (let y = 0; y < gridHeight; y++) {
        const idx = (y * width + x) * 4;
        pixelBuffer[idx] = 30;
        pixelBuffer[idx + 1] = 30;
        pixelBuffer[idx + 2] = 30;
        pixelBuffer[idx + 3] = 255;
      }
    }
  }

  function palette(i: number): [number, number, number] {
    return [Math.min(i * 8, 255), Math.min(i * 6, 255), Math.min(i * 12, 255)];
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
    image-rendering: pixelated;
  }
</style>
