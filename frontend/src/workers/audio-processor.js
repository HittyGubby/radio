class AudioProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.sampleRate = 24000;
    this.audioBuffer = new Float32Array(0);
    this.bufferPosition = 0;
    this.totalProcessed = 0;
    this.wasEmpty = true;

    this.port.onmessage = (event) => {
      if (event.data.type === 'addBuffer') {
        const samples = event.data.samples;
        const newBuffer = new Float32Array(this.audioBuffer.length + samples.length);
        newBuffer.set(this.audioBuffer);
        newBuffer.set(samples, this.audioBuffer.length);
        this.audioBuffer = newBuffer;

        if (this.wasEmpty && this.audioBuffer.length > 0) {
          this.wasEmpty = false;
          this.sendStats();
        }
      } else if (event.data.type === 'clear') {
        this.audioBuffer = new Float32Array(0);
        this.bufferPosition = 0;
        this.totalProcessed = 0;
        this.wasEmpty = true;
      }
    };
  }

  sendStats() {
    const bufferMs = (this.audioBuffer.length / this.sampleRate) * 1000;
    this.port.postMessage({
      type: 'stats',
      bufferSamples: this.audioBuffer.length,
      bufferMs: bufferMs.toFixed(2),
      totalProcessed: this.totalProcessed
    });
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    const channel = output[0];

    if (!channel) return true;

    for (let i = 0; i < channel.length; i++) {
      if (this.bufferPosition < this.audioBuffer.length) {
        channel[i] = this.audioBuffer[this.bufferPosition];
        this.bufferPosition++;
        this.totalProcessed++;
      } else {
        channel[i] = 0;
      }
    }

    if (this.bufferPosition > 0) {
      this.audioBuffer = this.audioBuffer.slice(this.bufferPosition);
      this.bufferPosition = 0;
    }

    if (this.totalProcessed % 128 === 0 && this.audioBuffer.length > 0) {
      this.sendStats();
    }

    return true;
  }
}

registerProcessor('audio-processor', AudioProcessor);