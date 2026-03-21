<script lang="ts">
  import type { SongMetadata } from "../metadata/Metadata.svelte";
  import { mergeLyricsAndTranslation } from "../metadata/Metadata.svelte";

  export let metadata: SongMetadata;
  export let playerProgress = 0;

  let container: HTMLElement;
  let lyrics: { time: number; text: string; translation: string | null }[] = [];
  let currentIndex = -1;
  let userScrolling = false;
  let scrollTimer: number | null = null;
  let lyricHash = "";

  $: {
    const h = metadata.lyric + "|" + metadata.tlyric;
    if (h !== lyricHash) {
      lyricHash = h;
      lyrics = mergeLyricsAndTranslation(metadata.lyric, metadata.tlyric) || [];
      currentIndex = -1;
    }
  }

  $: if (lyrics.length > 0 && playerProgress >= 0) {
    updateCurrentLyric();
  }

  function updateCurrentLyric() {
    let i = currentIndex;

    while (i + 1 < lyrics.length && playerProgress >= lyrics[i + 1].time) i++;
    while (i >= 0 && playerProgress < lyrics[i].time) i--;

    if (i !== currentIndex) {
      currentIndex = i;
      if (!userScrolling) centerActive();
    }
  }

  function centerActive() {
    const el = container?.children[currentIndex] as HTMLElement;
    if (!el) return;
    const offset = el.offsetTop - container.clientHeight / 2 + el.clientHeight / 2;
    container.scrollTo({
      top: offset,
      behavior: "smooth",
    });
  }

  function handleScroll() {
    userScrolling = true;

    if (scrollTimer) clearTimeout(scrollTimer);

    scrollTimer = window.setTimeout(() => {
      userScrolling = false;
      centerActive();
    }, 1500);
  }

  export function resetScroll() {
    currentIndex = -1;
    container?.scrollTo({ top: 0, behavior: "smooth" });
  }
</script>

<div class="floating-lyrics">
  {#if lyrics.length}
    <div class="container" bind:this={container} on:scroll={handleScroll}>
      {#each lyrics as lyric, i (lyric.time)}
        <div class="line {i === currentIndex ? 'active' : ''}">
          <p class="text">{lyric.text}</p>
          {#if lyric.translation}
            <p class="trans">{lyric.translation}</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .floating-lyrics {
    position: fixed;
    right: 0;
    top: 0;
    height: 100%;
    z-index: 10;
  }

  .container {
    overflow-y: auto;
    height: 100%;
    margin-right: 1ch;
    mask-image: linear-gradient(to bottom, transparent, black 10%, black 90%, transparent);
    scrollbar-width: none;
    padding-top: 50vh;
    padding-bottom: 50vh;
    box-sizing: border-box;
  }

  .container::-webkit-scrollbar {
    display: none;
  }

  .line {
    margin: 1ch;
    text-align: right;
    transition: all 0.3s ease;
    text-shadow: 2px 2px 2px rgb(0, 0, 0);
  }

  .text {
    margin: 0;
    font-size: 22px;
    color: rgba(255, 255, 255, 0.8);
    transition: all 0.3s ease;
  }

  .trans {
    margin: 0;
    font-size: 20px;
    color: rgba(255, 255, 255, 0.8);
    transition: all 0.3s ease;
  }

  .line.active .text {
    font-size: 26px;
    font-weight: 700;
  }

  .line.active .trans {
    font-size: 24px;
    font-weight: 700;
  }

  @media (max-width: 800px) {
    .floating-lyrics {
      display: none;
    }
  }
</style>
