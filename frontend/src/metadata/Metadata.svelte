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

    const translationMap = new Map<number, string>();
    for (const t of translations) {
      translationMap.set(t.time, t.text);
    }

    for (const l of lyrics) {
      merged.push({
        time: l.time,
        text: l.text,
        translation: translationMap.get(l.time) || null,
      });
    }
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
  let eventSource: EventSource | null = null;
  let reconnectTimer: number | null = null;
  let lastCall = 0;
  export let playerProgress: number = 0;
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

  onMount(async () => {
    fetchMetadata();
    lastCall = Date.now();
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
    if (Date.now() - lastCall >= 1000) {
      lastCall = Date.now();
      const INFO_URL = "/info";
      const response = await fetch(INFO_URL);
      metadata = await response.json();
      if (!eventSource) {
        connectSSE();
      }
    }
  }

  function connectSSE() {
    const STATUS_URL = "/status";
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
          playerProgress = parseFloat(event.data);
        } else setTimeout(async () => fetchMetadata(), 500);
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
