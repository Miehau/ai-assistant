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

<div
  class={`mb-2 p-3 rounded-lg ${type === "sent" ? "bg-primary text-primary-foreground ml-auto" : "bg-secondary text-secondary-foreground"} max-w-[80%]`}
>
  {@html parsedContent}
</div>
