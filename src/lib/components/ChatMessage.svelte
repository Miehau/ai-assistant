<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";

  export let type: "sent" | "received";
  export let content: string;

  let parsedContent: string;

  onMount(() => {
    marked.setOptions({
      breaks: true,
      gfm: true,
      renderer: new marked.Renderer()
    });
  });

  $: {
    (async () => {
      parsedContent = await marked(content);
    })();
  };
</script>

<div class={`flex mb-4 ${type === "sent" ? "bg-primary justify-end" : "bg-secondary"} w-full`}>
  <div class="w-8 flex-shrink-0 flex items-center justify-center">
    {#if type === "received"}
      <div class="w-8 h-8 rounded-full flex items-center justify-center">
        <span>🤖</span>
      </div>
    {/if}
  </div>
  <div class="flex-1 px-4 py-3 min-w-0 overflow-hidden">
    <div class="prose prose-sm max-w-none break-words whitespace-pre-wrap">
      <div class="markdown-content">
        {@html parsedContent}
      </div>
    </div>
  </div>
  <div class="w-8 flex-shrink-0 flex items-center justify-center">
    {#if type === "sent"}
      <div class="w-8 h-8 rounded-full bg-green-500 flex items-center justify-center">
        <span>👤</span>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Style for code blocks */
  :global(.markdown-content pre) {
    white-space: pre-wrap !important;
    word-wrap: break-word !important;
    overflow-x: auto;
    max-width: 100%;
  }

  :global(.markdown-content pre code) {
    white-space: pre-wrap !important;
    word-wrap: break-word !important;
    display: block;
  }

  :global(.markdown-content code) {
    font-size: 0.875em;
    background-color: rgba(0, 0, 0, 0.05);
    padding: 0.2em 0.4em;
    border-radius: 3px;
  }

  /* Ensure inline code also wraps properly */
  :global(.markdown-content p code) {
    white-space: pre-wrap !important;
    word-wrap: break-word !important;
  }
</style>
