<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";

  export let type: "sent" | "received";
  export let content: string;

  let parsedContent: string;

  onMount(() => {
    // Set options for marked to enable GitHub Flavored Markdown
    marked.setOptions({
      breaks: true, // Translate line breaks into <br>
      gfm: true, // Use GitHub Flavored Markdown
    });

    // Parse the content
  });

  $: {
    (async () => {
      parsedContent = await marked(content);
    })();
  };
</script>

<div class={`flex mb-4 ${type === "sent" ? "bg-primary justify-end" : "bg-secondary"}`}>
  <div class="w-14 flex-shrink-0 flex items-center justify-center">
    {#if type === "received"}
      <div class="w-8 h-8 rounded-full flex items-center justify-center">
        <span>🤖</span>
      </div>
    {/if}
  </div>
  <div class="flex-grow px-4 py-3 max-w-[calc(100%-7rem)]">
    {@html parsedContent}
  </div>
  <div class="w-14 flex-shrink-0 flex items-center justify-center">
    {#if type === "sent"}
      <div class="w-8 h-8 rounded-full bg-green-500 flex items-center justify-center">
        <span>👤</span>
      </div>
    {/if}
  </div>
</div>
