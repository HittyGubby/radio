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

  export interface MergedLyricLine {
    time: number;
    text: string;
    translation: string | null;
  }

  export function mergeLyricsAndTranslation(lyricText: string, tlyricText: string): MergedLyricLine[] {
    const lyrics = parseLyrics(lyricText);
    const translations = parseLyrics(tlyricText);

    const merged: MergedLyricLine[] = [];

    // Create a map of translations by time for quick lookup
    const translationMap = new Map<number, string>();
    for (const t of translations) {
      translationMap.set(t.time, t.text);
    }

    // Merge lyrics with their translations
    for (const l of lyrics) {
      merged.push({
        time: l.time,
        text: l.text,
        translation: translationMap.get(l.time) || null,
      });
    }

    // Sort by time
    merged.sort((a, b) => a.time - b.time);

    return merged;
  }

  export interface SongMetadata {
    status: string;
    name: string;
    singer: string;
    albumName: string;
    picUrl: string;
    progress: number;
    duration: number;
    lyric: string;
    tlyric: string;
  }
</script>

<script lang="ts">
  const STATUS_URL = "/status";

  let eventSource: EventSource | null = null;
  let reconnectTimer: number | null = null;
  let isRefreshing = false;
  let metadataWorker: Worker | null = null;

  export let metadata: SongMetadata = {
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

  export let playerProgress: number = 0;
  export let onSongChange: () => void = () => {};

  onMount(() => {
    // Initialize metadata worker
    metadataWorker = new Worker(new URL("../workers/metadata-worker.js", import.meta.url));
    metadataWorker.onmessage = (e) => {
      if (e.data.type === "success") {
        const newMetadata: SongMetadata = e.data.data;
        // Check if song changed BEFORE updating metadata
        const songChanged = newMetadata.name !== metadata.name ||
                           newMetadata.singer !== metadata.singer ||
                           newMetadata.albumName !== metadata.albumName;

        metadata = newMetadata;

        if (songChanged) {
          onSongChange();
        }

        isRefreshing = false;
      } else if (e.data.type === "error") {
        console.error("Failed to fetch metadata:", e.data.error);
        isRefreshing = false;
      }
    };

    fetchMetadata();
    if (!eventSource) {
      connectSSE();
    }
  });

  onDestroy(() => {
    if (eventSource) {
      eventSource.close();
    }
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
    }
    if (metadataWorker) {
      metadataWorker.terminate();
    }
  });

  function fetchMetadata() {
    if (isRefreshing || !metadataWorker) return;
    isRefreshing = true;
    metadataWorker.postMessage({ type: "fetch" });
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
            setTimeout(() => fetchMetadata(), 200);
          }
        } else {
          // Trigger onSongChange immediately for song changes
          onSongChange();
          // Fetch metadata after 200ms delay to give API time to be ready
          setTimeout(() => fetchMetadata(), 200);
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
    }, 3000);
  }
</script>
