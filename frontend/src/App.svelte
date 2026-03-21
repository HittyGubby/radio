<script lang="ts">
  import { onMount } from "svelte";
  import SpectrogramDrawer from "./spectrogram/SpectrogramDrawer.svelte";
  import AudioPlayer from "./audio/AudioPlayer.svelte";
  import Metadata, { type SongMetadata } from "./metadata/Metadata.svelte";
  import AudioPanel from "./components/AudioPanel.svelte";
  import FloatingLyrics from "./components/FloatingLyrics.svelte";

  let metadata: SongMetadata = {
    status: "",
    name: "",
    singer: "",
    albumName: "",
    picUrl: "",
    progress: 0,
    duration: 0,
    lyric: "",
    tlyric: "",
  };

  let playerProgress: number = 0;
  let audioContext: AudioContext | null = null;
  let isMobile: boolean = false;
  let audioPacketPerSec: number = 0;
  let audioBufferSamples: number = 0;
  let audioBufferCapacity: number = 0;
  let audioBitrate: number = 0;
  let audioSampleRate: number = 0;
  let audioPlayerComponent: any;
  let updateInterval: number = 50;
  let floatingLyricsComponent: any;

  function handleAudioContextReady(context: AudioContext) {
    audioContext = context;
  }

  function updateDebugData() {
    if (audioPlayerComponent) {
      audioPacketPerSec = audioPlayerComponent.getPacketPerSec?.() || 0;
      const stats = audioPlayerComponent.getAudioStats?.();
      audioSampleRate = stats.sampleRate;
      audioBitrate = stats.bitrate;
      audioBufferSamples = stats.jitterBufferSamples;
      audioBufferCapacity = stats.jitterBufferCapacity;
    }
  }

  onMount(() => {
    window.setInterval(updateDebugData, updateInterval);
  });
</script>

<main>
  <SpectrogramDrawer />
  <AudioPlayer bind:this={audioPlayerComponent} on:audioContextReady={(e) => handleAudioContextReady(e.detail)} />
  <Metadata bind:metadata bind:playerProgress />

  {#if !isMobile}
    <FloatingLyrics bind:this={floatingLyricsComponent} {metadata} {playerProgress} />
  {/if}

  <div class="debug-panel-container">
    <AudioPanel {metadata} {playerProgress} {audioPacketPerSec} {audioBufferSamples} {audioBufferCapacity} {audioBitrate} {audioSampleRate} />
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: #000;
    color: #fff;
    overflow: hidden;
  }

  .debug-panel-container {
    position: fixed;
    display: flex;
    top: 20px;
    left: 20px;
    width: calc(100vw - 40px);
    bottom: 20px;
    background: rgba(0, 0, 0, 0.6);
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-radius: 16px;
    backdrop-filter: blur(20px);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    overflow: hidden;
    z-index: 100;
  }

  @media (min-width: 800px) {
    .debug-panel-container {
      width: 300px;
    }
  }
</style>
