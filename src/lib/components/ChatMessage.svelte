<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import { shell } from "@tauri-apps/api";
  import type { Link } from "marked";
  import { Check, Copy } from "lucide-svelte";

  export let type: "sent" | "received";
  export let content: string;

  let parsedContent: string;

  onMount(() => {
    const renderer = new marked.Renderer();
    
    // Fix the code block renderer to use the correct type signature
    renderer.code = ({ text, lang, escaped }: { text: string, lang: string | undefined, escaped: boolean }) => {
      const code = escaped ? text : escapeHtml(text);
      return `
        <div class="code-block-wrapper relative group">
          <pre><code class="language-${lang || ''}">${code}</code></pre>
          <button
            class="copy-button opacity-0 group-hover:opacity-100 absolute top-2 right-2 
                   p-1.5 rounded-md bg-background/90 hover:bg-background 
                   shadow-sm border border-border transition-all duration-200"
            data-copy="${encodeURIComponent(text)}"
          >
            <svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
          </button>
        </div>
      `;
    };

    // Helper function to escape HTML
    function escapeHtml(text: string): string {
      return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
    }

    // Fix the link renderer to use the correct type signature
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

  function getCopyIcon() {
    return `<svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>`;
  }

  function getCheckIcon() {
    return `<svg class="w-4 h-4 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"></polyline>
            </svg>`;
  }

  async function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    
    // Handle copy button clicks
    const copyButton = target.closest('.copy-button');
    if (copyButton) {
      event.preventDefault();
      event.stopPropagation();
      const code = copyButton.getAttribute('data-copy');
      if (code) {
        try {
          await navigator.clipboard.writeText(decodeURIComponent(code));
          const icon = copyButton.querySelector('svg');
          if (icon) {
            // Show check icon
            icon.outerHTML = getCheckIcon();
            
            // Reset to copy icon after 2 seconds
            setTimeout(() => {
              const updatedIcon = copyButton.querySelector('svg');
              if (updatedIcon) {
                updatedIcon.outerHTML = getCopyIcon();
              }
            }, 2000);
          }
        } catch (err) {
          console.error('Failed to copy:', err);
        }
      }
      return;
    }

    // Handle link clicks
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
    rounded-lg px-4 py-2 max-w-[80%] relative
    ${type === "sent" 
      ? "bg-primary text-primary-foreground" 
      : "bg-muted"
    }`}
  >
    <div class="prose prose-sm dark:prose-invert max-w-none">
      <div class="markdown-content" on:click={handleClick}>
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
    background-color: hsl(var(--muted));
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    padding: 1rem;
    overflow-x: auto;
    margin: 0.5rem 0;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }

  :global(.markdown-content pre code) {
    color: rgb(var(--foreground));
    font-size: 0.875rem;
    line-height: 1.5;
    white-space: pre-wrap;
    background-color: transparent;
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

  :global(.markdown-content) {
    position: relative;
  }

  :global(.copy-button) {
    z-index: 10;
  }

  :global(.copy-button:focus) {
    opacity: 1;
  }

  :global(.code-block-wrapper) {
    position: relative;
    margin: 1rem 0;
  }

  :global(.code-block-wrapper:hover .copy-button) {
    opacity: 1;
  }

  /* When inside a received message (which has muted background) */
  :global(.bg-muted .markdown-content pre) {
    background-color: hsl(var(--background));
  }

  /* When inside a sent message (which has primary background) */
  :global(.bg-primary .markdown-content pre) {
    background-color: hsl(var(--primary-foreground));
    border-color: rgba(255, 255, 255, 0.2);
  }
</style>
