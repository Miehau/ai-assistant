<script lang="ts">
  import type { Attachment } from "$lib/types";
  import { marked } from "marked";
  import { onMount } from "svelte";
  import type { Code } from 'marked';
  
  export let type: "sent" | "received";
  export let content: string;
  export let attachments: (Attachment)[] | undefined = undefined;

  onMount(() => {
    const renderer = new marked.Renderer();
    
    renderer.code = ({ text, lang }: Code) => {
      const code = escapeHtml(text || '');
      return `
        <div class="code-block-wrapper relative group">
          <div class="absolute top-0 left-0 right-0 h-8 bg-gradient-to-r from-primary/10 to-transparent rounded-t-lg flex items-center px-3">
            <div class="flex items-center gap-2">
              <span class="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">${lang || 'text'}</span>
            </div>
          </div>
                 <button
            class="copy-button opacity-0 group-hover:opacity-100 absolute top-1 right-2 
            p-1.5 rounded-md hover:bg-background/50 transition-all duration-200"
            data-copy="${encodeURIComponent(text || '')}"
          >
            <svg class="w-3.5 h-3.5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
          </button>
          <div class="mt-8">
            <pre><code class="language-${lang || ''}">${code}</code></pre>
          </div>
        </div>
      `;
    };

    function escapeHtml(text: string): string {
      return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
    }

    marked.setOptions({
      breaks: true,
      gfm: true,
      renderer: renderer
    });
  });

  async function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    const copyButton = target.closest('.copy-button');
    
    if (copyButton) {
      event.preventDefault();
      const code = copyButton.getAttribute('data-copy');
      if (code) {
        try {
          await navigator.clipboard.writeText(decodeURIComponent(code));
          const icon = copyButton.querySelector('svg');
          if (icon) {
            icon.outerHTML = `<svg class="w-4 h-4 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"></polyline>
            </svg>`;
            
            setTimeout(() => {
              const updatedIcon = copyButton.querySelector('svg');
              if (updatedIcon) {
                updatedIcon.outerHTML = `<svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>`;
              }
            }, 2000);
          }
        } catch (err) {
          console.error('Failed to copy:', err);
        }
      }
    }
  }

  $: htmlContent = marked(content);
</script>

<div class="flex gap-3 {type === 'received' ? 'justify-start' : 'justify-end'}">
  <div class="rounded-2xl px-4 py-2 w-[75%] {type === 'received' ? 'bg-muted' : 'text-primary-foreground bg-primary/30'}">
    <div class="prose prose-sm dark:prose-invert max-w-none">
      <div class="markdown-content" on:click={handleClick}>
        {@html htmlContent}
      </div>
      {#if attachments && attachments.length > 0}
        <div class="mt-2 space-y-2">
          {#each attachments as attachment}
            {#if attachment.attachment_type === 'image'}
              <img 
                src={attachment.data} 
                alt={attachment.name}
                class="max-w-full rounded-xl"
              />
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  </div>
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
    background-color: hsl(var(--background));
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    padding: 0.25rem 0.75rem 0.75rem;
    overflow-x: auto;
    margin: 0.5rem 0;
  }

  :global(.markdown-content pre code) {
    color: rgb(var(--foreground));
    font-size: 0.875rem;
    line-height: 1.5;
    white-space: pre-wrap;
    background-color: transparent;
    display: block;
    padding-top: 0.5rem;
  }

  /* Inline code styles */
  :global(.markdown-content code:not(pre code)) {
    background-color: rgb(var(--muted) / 0.5);
    padding: 0.2em 0.4em;
    border-radius: 0.25rem;
    font-size: 0.875em;
  }

  /* Copy button styles */
  :global(.copy-button) {
    z-index: 10;
  }

  :global(.copy-button:focus) {
    opacity: 1;
  }

  :global(.code-block-wrapper) {
    position: relative;
    margin: 1rem 0;
    border-radius: 0.5rem;
    overflow: hidden;
    transition: all 150ms ease;
    background-color: hsl(var(--background));
    border: 1px solid hsl(var(--border));
  }

  :global(.code-block-wrapper:hover) {
    border-color: hsl(var(--primary) / 0.5);
  }

  :global(.code-block-wrapper .copy-button) {
    opacity: 1;
  }

  /* Adjust backgrounds for different message types */
  :global(.bg-muted .markdown-content pre) {
    background-color: hsl(var(--background));
  }

  :global(.bg-primary .markdown-content pre) {
    background-color: hsl(var(--background));
    border-color: rgba(255, 255, 255, 0.1);
  }

  :global(.bg-primary .code-block-wrapper) {
    background-color: hsl(var(--background));
    border-color: rgba(255, 255, 255, 0.1);
  }

  :global(.bg-primary .code-block-wrapper .bg-gradient-to-r) {
    background: linear-gradient(to right, rgba(255, 255, 255, 0.1), transparent);
  }
</style>

