// Web Worker for spectrogram FFT processing
// This handles all audio frequency analysis and color palette generation
// off the main thread to improve performance

let colorPalette = [];

// Generate color palette for spectrogram visualization
function generateColorPalette() {
  colorPalette = [];
  for (let i = 0; i < 256; i++) {
    const intensity = i / 255;

    // Darker, more modern SDR palette
    // Deep black -> dark purple -> blue -> cyan -> white
    let r, g, b;

    if (intensity < 0.3) {
      // Black to very dark purple
      const t = intensity / 0.3;
      r = Math.round(5 + t * 15);
      g = Math.round(5 + t * 5);
      b = Math.round(15 + t * 30);
    } else if (intensity < 0.7) {
      // Dark purple to dark blue
      const t = (intensity - 0.3) / 0.4;
      r = Math.round(20 + t * 10);
      g = Math.round(10 + t * 30);
      b = Math.round(45 + t * 60);
    } else if (intensity < 0.9) {
      // Dark blue to blue-cyan
      const t = (intensity - 0.7) / 0.2;
      r = Math.round(30 + t * 10);
      g = Math.round(40 + t * 70);
      b = Math.round(105 + t * 40);
    } else {
      // Blue-cyan to cyan
      const t = (intensity - 0.9) / 0.1;
      r = Math.round(40 + t * 60);
      g = Math.round(110 + t * 60);
      b = Math.round(145 + t * 10);
    }

    colorPalette.push([r, g, b]);
  }
}

// Process frequency data into image data
function processFrequencyData(frequencyData, maxBins, canvasWidth, canvasHeight, columnWidth) {
  const imageData = {
    data: new Uint8ClampedArray(columnWidth * canvasHeight * 4),
    width: columnWidth,
    height: canvasHeight
  };

  for (let y = 0; y < canvasHeight; y++) {
    // Use logarithmic scaling for better frequency distribution
    const normalizedY = y / canvasHeight;
    const binIndex = Math.floor(Math.pow(normalizedY, 1.5) * maxBins);

    const intensity = frequencyData[binIndex];
    const [r, g, b] = colorPalette[intensity];

    for (let x = 0; x < columnWidth; x++) {
      const pixelIndex = (y * columnWidth + x) * 4;
      imageData.data[pixelIndex] = r;
      imageData.data[pixelIndex + 1] = g;
      imageData.data[pixelIndex + 2] = b;
      imageData.data[pixelIndex + 3] = 255;
    }
  }

  return imageData;
}

// Handle messages from main thread
self.onmessage = function (e) {
  const { type, data } = e.data;

  if (type === 'init') {
    generateColorPalette();
    self.postMessage({ type: 'ready' });
  } else if (type === 'process') {
    const { frequencyData, maxBins, canvasWidth, canvasHeight, columnWidth } = data;
    const imageData = processFrequencyData(frequencyData, maxBins, canvasWidth, canvasHeight, columnWidth);
    self.postMessage({ type: 'result', data: imageData });
  }
};

// Initialize on load
generateColorPalette();