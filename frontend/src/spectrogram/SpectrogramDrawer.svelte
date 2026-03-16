<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  const COLUMN_WIDTH = 3;

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
  let smoothedFrame: Uint8Array | null = null;

  const SPECTRO_URL = "/spectro";

  let animationId: number;

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
      magFilter: "linear",
      minFilter: "linear",
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

      if (frames.length > 200) frames.splice(0, frames.length - 200);
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
    if (frames.length === 0) return;

    const frame = frames.shift()!;

    scroll();

    drawColumn(frame);
  }

  function scroll() {
    const shift = COLUMN_WIDTH;

    for (let y = 0; y < height; y++) {
      const row = y * width * 4;

      for (let x = 0; x < width - shift; x++) {
        const src = row + (x + shift) * 4;
        const dst = row + x * 4;

        pixelBuffer[dst] = pixelBuffer[src];
        pixelBuffer[dst + 1] = pixelBuffer[src + 1];
        pixelBuffer[dst + 2] = pixelBuffer[src + 2];
        pixelBuffer[dst + 3] = 255;
      }
    }
  }

  function drawColumn(frame: Uint8Array) {
    // Apply temporal smoothing
    let processedFrame: Uint8Array;
    if (smoothedFrame === null) {
      processedFrame = new Uint8Array(frame);
      smoothedFrame = new Uint8Array(frame);
    } else {
      // Ensure arrays are same length
      if (smoothedFrame.length !== frame.length) {
        smoothedFrame = new Uint8Array(frame);
        processedFrame = new Uint8Array(frame);
      } else {
        // Exponential smoothing: smoothed = alpha * current + (1 - alpha) * previous
        // Alpha of 0.7 means 70% current, 30% previous (adjust based on preference)
        const alpha = 0.5;
        processedFrame = new Uint8Array(frame.length);
        for (let i = 0; i < frame.length; i++) {
          processedFrame[i] = Math.round(alpha * frame[i] + (1 - alpha) * smoothedFrame[i]);
        }
      }
    }

    // Update smoothed frame for next iteration
    smoothedFrame.set(processedFrame);

    const x = width - COLUMN_WIDTH;

    const bins = processedFrame.length;

    // First, compute all pixel values for this column
    const columnPixels: [number, number, number][] = [];
    for (let y = 0; y < height; y++) {
      const ny = y / height;

      const exact = Math.pow(ny, 1.5) * bins;

      const lo = Math.floor(exact);
      const hi = Math.min(lo + 1, bins - 1);

      const f = exact - lo;

      const v = (processedFrame[lo] ?? 0) * (1 - f) + (processedFrame[hi] ?? 0) * f;

      columnPixels.push(palette(v));
    }

    // Draw the column with cross-column blending
    for (let y = 0; y < height; y++) {
      const [r, g, b] = columnPixels[y];

      for (let px = 0; px < COLUMN_WIDTH; px++) {
        const idx = (y * width + x + px) * 4;

        // Apply cross-column blending at the left edge of the column
        // (except for the very first pixel of the column which should be sharp)
        if (px === 0 && x > 0) {
          // Get the previous column's pixel
          const prevIdx = (y * width + x - 1) * 4;
          const prevR = pixelBuffer[prevIdx];
          const prevG = pixelBuffer[prevIdx + 1];
          const prevB = pixelBuffer[prevIdx + 2];

          // Blend with 50% of previous column for smooth transition
          const blendFactor = 0.5;
          pixelBuffer[idx] = Math.round(r * (1 - blendFactor) + prevR * blendFactor);
          pixelBuffer[idx + 1] = Math.round(g * (1 - blendFactor) + prevG * blendFactor);
          pixelBuffer[idx + 2] = Math.round(b * (1 - blendFactor) + prevB * blendFactor);
          pixelBuffer[idx + 3] = 255;
        } else {
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
  }
</style>
