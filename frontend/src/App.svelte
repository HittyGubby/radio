<script lang="ts">
  import Spectrogram from "./spectrogram/Spectrogram.svelte";
  import AudioPlayer from "./audio/AudioPlayer.svelte";
  import Metadata, { type SongMetadata, parseLyrics } from "./metadata/Metadata.svelte";

  let metadata: SongMetadata = {
    status: "",
    name: "",
    singer: "",
    albumName: "",
    picUrl: "",
    progress: 0,
    duration: 0,
    playbackRate: 1,
    lyric: "",
    tlyric: "",
  };

  let playerProgress: number = 0;
  let audioLatency: number = 0;
  let packetCount: number = 0;
  let currentLyric: string = "";
  let lyricIndex: number = -1;
  let updateInterval: number;

  function handleSongChange() {
    lyricIndex = -1;
    currentLyric = "";
  }

  function handleStateChange(progress: number) {
    playerProgress = progress;
    updateLyric();
  }

  function handleLatencyUpdate(latency: number) {
    audioLatency = latency;
  }

  function updateLyric() {
    if (!metadata.lyric) return;
    const lyrics = parseLyrics(metadata.lyric);
    let newIndex = -1;
    for (let i = lyrics.length - 1; i >= 0; i--) {
      if (playerProgress >= lyrics[i].time) {
        newIndex = i;
        break;
      }
    }
    if (newIndex !== lyricIndex && newIndex >= 0) {
      lyricIndex = newIndex;
      currentLyric = lyrics[newIndex].text;
    }
  }

  function formatTime(seconds: number): string {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  function startMetricsUpdate() {
    updateInterval = setInterval(() => {
      const prevCount = packetCount;
    }, 1000);
  }

  function stopMetricsUpdate() {
    if (updateInterval) {
      clearInterval(updateInterval);
    }
  }

  startMetricsUpdate();
</script>

<main>
  <Spectrogram />
  <AudioPlayer on:latencyUpdate={(e) => handleLatencyUpdate(e.detail)} />
  <Metadata bind:metadata bind:playerProgress onSongChange={handleSongChange} onStateChange={handleStateChange} />

  <div class="debug-panel">
    <div class="debug-section">
      <h3>Song Info</h3>
      <div><strong>Name:</strong> {metadata.name || "N/A"}</div>
      <div><strong>Singer:</strong> {metadata.singer || "N/A"}</div>
      <div><strong>Album:</strong> {metadata.albumName || "N/A"}</div>
      <div><strong>Status:</strong> {metadata.status || "N/A"}</div>
      <div><strong>Progress:</strong> {formatTime(playerProgress)} / {formatTime(metadata.duration)}</div>
    </div>
    <div class="debug-section">
      <h3>Audio Stats</h3>
      <div><strong>Latency:</strong> {audioLatency.toFixed(2)} ms</div>
      <div><strong>Buffer:</strong> {metadata.status}</div>
    </div>
    {#if currentLyric}
      <div class="debug-section">
        <h3>Lyrics</h3>
        <div class="lyric">{currentLyric}</div>
      </div>
    {/if}
    {#if metadata.picUrl}
      <div class="debug-section">
        <h3>Album Cover</h3>
        <img src={metadata.picUrl} alt="Album Cover" />
      </div>
    {/if}
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
    background: #000;
    color: #fff;
    overflow: hidden;
  }

  .debug-panel {
    position: fixed;
    top: 20px;
    left: 20px;
    width: 300px;
    max-height: 90%;
    overflow-y: auto;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    padding: 16px;
    z-index: 1000;
    backdrop-filter: blur(10px);
  }

  .debug-section {
    margin-bottom: 16px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .debug-section:last-child {
    margin-bottom: 0;
    padding-bottom: 0;
    border-bottom: none;
  }

  .debug-section h3 {
    margin: 0 0 8px 0;
    font-size: 14px;
    font-weight: 600;
    color: #fff;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .debug-section div {
    margin: 4px 0;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
    text-align: left;
  }

  .debug-section strong {
    color: #fff;
  }

  .debug-section img {
    width: 100%;
    border-radius: 4px;
    margin-top: 8px;
  }

  .lyric {
    font-size: 14px;
    color: #00ff88;
    padding: 8px;
    background: rgba(0, 255, 136, 0.1);
    border-radius: 4px;
  }
</style>
