<script lang="ts">
  import type { SongMetadata } from "../metadata/Metadata.svelte";

  export let audioPacketPerSec: number = 0;
  export let audioBufferSamples: number = 0;
  export let audioBufferCapacity: number = 0;
  export let audioBitrate: number = 0;
  export let audioSampleRate: number = 0;
  export let metadata: SongMetadata;
  export let playerProgress: number = 0;
  export let showDebug: boolean = false;

  function formatTime(seconds: number): string {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return bytes + " B/s";
    else if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(2) + " KB/s";
    else if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(2) + " MB/s";
    else return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB/s";
  }

  function formatbitrate(bps: number): string {
    return (bps / 1000).toFixed(2) + " Kbps";
  }
</script>

<div class="audio-panel">
  <div class="song-info">
    {#if metadata.name}
      <p class="song-name">{metadata.name}</p>
    {/if}
    {#if metadata.singer}
      <p class="artist-name">{metadata.singer}</p>
    {/if}
    {#if metadata.albumName}
      <p class="album-name">{metadata.albumName}</p>
    {/if}
  </div>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="album-cover" on:click={() => (showDebug = !showDebug)}>
    {#if !showDebug}
      {#if metadata.picUrl}
        <img src={metadata.picUrl} alt="Album Cover" />
      {:else}
        <div class="placeholder">
          <p>ХитыГабы <br /> Радио</p>
        </div>
      {/if}
    {:else}
      <div class="debug-section">
        <div class="debug-row">
          <span class="label">Packet/s:</span>
          <span class="value">{audioPacketPerSec}</span>
        </div>
        <div class="debug-row">
          <span class="label">Throughput:</span>
          <span class="value">{((audioPacketPerSec * audioBitrate) / 48000).toFixed(2)} Kbps</span>
        </div>
        <div class="debug-row">
          <span class="label">Buffer:</span>
          <span class="value">{audioBufferSamples} / {audioBufferCapacity}</span>
        </div>
        <div class="debug-row">
          <span class="label">Sample Rate:</span>
          <span class="value">{audioSampleRate}</span>
        </div>
        <div class="debug-row">
          <span class="label">Bitrate:</span>
          <span class="value">{(audioBitrate / 1000).toFixed(2)} Kbps</span>
        </div>
      </div>
    {/if}
  </div>

  <div class="controls">
    <div class="progress-container">
      <div class="progress-bar">
        <div class="progress-fill" style="width: {(playerProgress / metadata.duration) * 100}%"></div>
      </div>
      <div class="progress-text">
        <span class="current-time">{formatTime(playerProgress)}</span>
        <span class="total-time">{formatTime(metadata.duration)}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .audio-panel {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 20px;
    width: 100%;
    align-items: stretch;
  }

  .song-info {
    text-align: left;
    font-weight: 500;
  }

  .song-name {
    font-size: 16px;
    margin: 0;
    color: rgba(255, 255, 255, 1);
  }

  .artist-name {
    font-size: 13px;
    margin: 0;
    color: rgba(255, 255, 255, 0.8);
  }

  .album-name {
    font-size: 13px;
    margin: 0;
    color: rgba(255, 255, 255, 0.6);
  }

  .album-cover {
    width: 80%;
    max-height: 80%;
    aspect-ratio: 1 / 1;
    border-radius: 16px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);

    border: 2px solid rgba(255, 255, 255, 0.2);
    margin: auto;
  }

  .album-cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .placeholder {
    justify-content: center;
    align-content: center;
    background: #222222;
    color: rgba(255, 255, 255, 0.2);
    width: 100%;
    height: 100%;
    font-size: 32px;
    margin: auto;
  }

  .progress-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-grow: 1;
  }

  .progress-bar {
    width: 100%;
    height: 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
    border-radius: 3px;
  }

  .progress-text {
    display: flex;
    justify-content: space-between;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.6);
    font-weight: 500;
  }

  .debug-section {
    width: 100%;
    height: 100%;
    background: rgba(255, 255, 255, 0.03);
    object-fit: cover;
    display: flex;
    flex-direction: column;
    padding: 20px;
    box-sizing: border-box;
  }

  .debug-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    flex: 1;
  }

  .debug-row:last-child {
    border-bottom: none;
  }

  .label {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
    font-weight: 400;
  }

  .value {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.9);
    font-weight: 500;
  }
</style>
