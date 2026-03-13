<script lang="ts" context="module">
  import { onMount, onDestroy } from "svelte";

  export function parseLyrics(lyricText: string): Array<{ time: number; text: string }> {
    const lines = lyricText.split("\n");
    const lyrics: Array<{ time: number; text: string }> = [];
    for (const line of lines) {
      const match = line.match(/\[(\d{2}):(\d{2})\.(\d{2,3})\](.*)/);
      if (match) {
        const minutes = parseInt(match[1]);
        const seconds = parseInt(match[2]);
        const milliseconds = parseInt(match[3].padEnd(3, "0"));
        const time = minutes * 60 + seconds + milliseconds / 1000;
        const text = match[4].trim();
        if (text) {
          lyrics.push({ time, text });
        }
      }
    }
    return lyrics;
  }

  export interface SongMetadata {
    status: string;
    name: string;
    singer: string;
    albumName: string;
    picUrl: string;
    progress: number;
    duration: number;
    playbackRate: number;
    lyric: string;
    tlyric: string;
  }
</script>

<script lang="ts">
  const INFO_URL = "/info";
  const STATUS_URL = "/status";

  let eventSource: EventSource | null = null;
  let reconnectTimer: number | null = null;
  let isRefreshing = false;

  export let metadata: SongMetadata = {
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

  export let playerProgress: number = 0;
  export let onSongChange: () => void = () => {};
  export let onStateChange: (progress: number) => void = () => {};

  onMount(() => {
    fetchMetadata();
    setTimeout(() => {
      if (!eventSource) {
        connectSSE();
      }
    }, 2000);
  });

  onDestroy(() => {
    if (eventSource) {
      eventSource.close();
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
  });

  async function fetchMetadata() {
    if (isRefreshing) return;
    isRefreshing = true;

    try {
      const response = await fetch(INFO_URL);
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      const newMetadata: SongMetadata = await response.json();
      if (newMetadata.name !== metadata.name || newMetadata.singer !== metadata.singer || newMetadata.albumName !== metadata.albumName) {
        onSongChange();
      }
      metadata = newMetadata;
    } catch (error) {
      console.error("Failed to fetch metadata:", error);
    } finally {
      isRefreshing = false;
    }
  }

  function connectSSE() {
    eventSource = new EventSource(STATUS_URL);
    eventSource.onopen = () => {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    for (const eventType of ["progress", "status", "name", "singer", "albumName"]) {
      eventSource.addEventListener(eventType, (event: MessageEvent) => {
        if (eventType === "progress") {
          const progress = parseFloat(event.data);
          playerProgress = progress;
          if (metadata.duration > 0 && progress >= metadata.duration) {
            fetchMetadata();
          }
          onStateChange(progress);
        } else {
          fetchMetadata();
        }
      });
    }

    eventSource.onerror = (error) => {
      console.error("SSE error:", error);
      eventSource?.close();
      scheduleReconnect();
    };
  }

  function scheduleReconnect() {
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connectSSE();
    }, 5000);
  }
</script>
