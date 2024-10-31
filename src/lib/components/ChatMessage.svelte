<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import { shell } from "@tauri-apps/api";
  import type { Link } from "marked";

  export let type: "sent" | "received";
  export let content: string;

  let parsedContent: string;

  onMount(() => {
    const renderer = new marked.Renderer();
    
    renderer.link = ({ href, title, text }: Link) => {
      if (!href) return text;
      return `<a href="#" 
                data-href="${href}"
                title="${title || ''}" 
                class="cursor-pointer">${text}</a>`;
    };

    marked.setOptions({
      breaks: true,
      gfm: true,
      renderer: renderer
    });
  });

  async function handleLinkClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (target.tagName.toLowerCase() === 'a') {
      event.preventDefault();
      const href = target.getAttribute('data-href');
      if (href) {
        try {
          await shell.open(href);
        } catch (error) {
          console.error('Failed to open link:', error);
        }
      }
    }
  }

  $: {
    (async () => {
      parsedContent = await marked(content);
    })();
  };
</script>

<div class={`flex gap-3 ${type === "sent" ? "justify-end" : "justify-start"}`}>
  {#if type === "received"}
    <div class="w-8 h-8 rounded-full flex items-center justify-center bg-primary/10">
      <span>🤖</span>
    </div>
  {/if}
  
  <div class={`
    rounded-lg px-4 py-2 max-w-[80%] 
    ${type === "sent" 
      ? "bg-primary text-primary-foreground" 
      : "bg-muted"
    }`}
  >
    <div class="prose prose-sm dark:prose-invert max-w-none">
      <div class="markdown-content" on:click={handleLinkClick}>
        {@html parsedContent}
      </div>
    </div>
  </div>

  {#if type === "sent"}
    <div class="w-8 h-8 rounded-full flex items-center justify-center bg-primary/10">
      <span>👤</span>
    </div>
  {/if}
</div>

<style>
  /* Base markdown styles */
  :global(.markdown-content p) {
    line-height: 1.5;
  }
  
  :global(.markdown-content p:last-child) {
    margin-bottom: 0;
  }

  /* Code block styles */
  :global(.markdown-content pre) {
    background-color: rgb(var(--background) / 0.8);
    border-radius: 0.5rem;
    padding: 1rem;
    overflow-x: auto;
  }

  :global(.markdown-content pre code) {
    color: rgb(var(--foreground));
    font-size: 0.875rem;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  /* Inline code styles */
  :global(.markdown-content code:not(pre code)) {
    background-color: rgb(var(--muted) / 0.5);
    padding: 0.2em 0.4em;
    border-radius: 0.25rem;
    font-size: 0.875em;
  }

  /* List styles */
  :global(.markdown-content ul),
  :global(.markdown-content ol) {
    padding-left: 1.5rem;
    margin: 0.5rem 0;
  }

  :global(.markdown-content li) {
    margin: 0.25rem 0;
  }

  /* Link styles */
  :global(.markdown-content a) {
    color: rgb(var(--primary));
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  :global(.markdown-content a:hover) {
    text-decoration: none;
    opacity: 0.8;
  }

  /* Table styles */
  :global(.markdown-content table) {
    width: 100%;
    border-collapse: collapse;
    margin: 0.5rem 0;
  }

  :global(.markdown-content th),
  :global(.markdown-content td) {
    padding: 0.5rem;
    border: 1px solid rgb(var(--border));
    text-align: left;
  }

  :global(.markdown-content th) {
    background-color: rgb(var(--muted));
  }

  /* Blockquote styles */
  :global(.markdown-content blockquote) {
    border-left: 3px solid rgb(var(--primary));
    padding-left: 1rem;
    font-style: italic;
  }

  /* Add header styles */
  :global(.markdown-content h1),
  :global(.markdown-content h2),
  :global(.markdown-content h3),
  :global(.markdown-content h4),
  :global(.markdown-content h5),
  :global(.markdown-content h6) {
    font-weight: 600;
    line-height: 1.25;
    margin-top: 1.5rem;
    margin-bottom: 1rem;
  }

  :global(.markdown-content h1) {
    font-size: 1.5em;
  }

  :global(.markdown-content h2) {
    font-size: 1.25em;
  }

  :global(.markdown-content h3) {
    font-size: 1.125em;
  }

  :global(.markdown-content h4) {
    font-size: 1em;
  }

  :global(.markdown-content h5) {
    font-size: 0.875em;
  }

  :global(.markdown-content h6) {
    font-size: 0.85em;
  }

  /* Adjust first header margin */
  :global(.markdown-content > *:first-child) {
    margin-top: 0;
  }
</style>
