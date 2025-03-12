<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import Prism from 'prismjs';
  import 'prismjs/themes/prism-tomorrow.css';
  import 'prismjs/components/prism-javascript';
  import 'prismjs/components/prism-json';
  import 'prismjs/components/prism-typescript';
  import 'prismjs/components/prism-python';
  import 'prismjs/components/prism-bash';
  import 'prismjs/components/prism-markdown';
  import 'prismjs/components/prism-java';
  import 'prismjs/components/prism-kotlin';
  import 'prismjs/components/prism-rust';
  import 'prismjs/components/prism-sql';
  import 'prismjs/components/prism-mermaid';
  import 'prismjs/components/prism-typescript';
  import 'prismjs/components/prism-git';
  import 'prismjs/components/prism-docker';
  import 'prismjs/components/prism-csv';
  import type { Attachment } from "$lib/types";

  export let type: "sent" | "received";
  export let content: string;
  export let attachments: Attachment[] | undefined = undefined;

  interface RendererCode {
    text?: string;
    lang?: string;
  }

  function escapeHtml(text: string): string {
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  onMount(async () => {
    const renderer = new marked.Renderer();

    renderer.code = ({ text, lang }: RendererCode) => {
      const code = text || "";
      const language = lang || "text";
      const highlightedCode = language ? Prism.highlight(
        code,
        Prism.languages[language] || Prism.languages.text,
        language
      ) : escapeHtml(code);
      
      return `
        <div class="code-block-wrapper relative group">
          <div class="absolute top-0 left-0 right-0 h-8 bg-zinc-800 rounded-t-lg flex items-center px-3">
            <div class="flex items-center gap-2">
              <span class="text-[10px] uppercase tracking-wider text-zinc-400 font-medium">${language}</span>
            </div>
          </div>
          <button
            class="copy-button opacity-0 group-hover:opacity-100 absolute top-1 right-2 
            p-1.5 rounded-md hover:bg-zinc-700 transition-all duration-200"
            data-copy="${encodeURIComponent(code)}"
          >
            <svg class="w-3.5 h-3.5 text-zinc-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
          </button>
          <div class="mt-8">
            <pre class="!bg-zinc-900 !border-zinc-700"><code class="language-${language}">${highlightedCode}</code></pre>
          </div>
        </div>
      `;
    };

    // Make links open in a new window
    renderer.link = function({href, title, text}) {
      const titleAttr = title ? ` title="${title}"` : '';
      return `<a href="${href}" target="_blank" rel="noopener noreferrer"${titleAttr}>${text}</a>`;
    };

    marked.setOptions({
      breaks: true,
      gfm: true,
      renderer: renderer,
    });
  });

  // For sent messages, we'll let marked handle the escaping
  $: htmlContent = marked(content);

  async function handleInteraction(event: MouseEvent | KeyboardEvent) {
    // Only handle Enter or Space key for keyboard events
    if (event instanceof KeyboardEvent && event.key !== 'Enter' && event.key !== ' ') {
      return;
    }

    const target = event.target as HTMLElement;
    const copyButton = target.closest(".copy-button");

    if (copyButton) {
      event.preventDefault();
      const code = copyButton.getAttribute("data-copy");
      if (code) {
        try {
          await navigator.clipboard.writeText(decodeURIComponent(code));
          const icon = copyButton.querySelector("svg");
          if (icon) {
            icon.outerHTML = `<svg class="w-4 h-4 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12"></polyline>
            </svg>`;

            setTimeout(() => {
              const updatedIcon = copyButton.querySelector("svg");
              if (updatedIcon) {
                updatedIcon.outerHTML = `<svg class="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>`;
              }
            }, 2000);
          }
        } catch (err) {
          console.error("Failed to copy:", err);
        }
      }
    }
  }

  function getPreviewText(text: string): string {
    const words = text.split(/\s+/);
    if (words.length <= 15) return text;
    return words.slice(0, 15).join(' ') + ' ...';
  }
</script>

<div class="flex gap-3 {type === 'received' ? 'justify-start' : 'justify-end'}">
  <div
    class="rounded-2xl px-4 py-2 max-w-[75%] {type === 'received'
      ? 'bg-muted'
      : 'text-primary-foreground bg-primary/30'}"
  >
    <div class="prose prose-sm dark:prose-invert max-w-none">
      <div 
        class="markdown-content" 
        on:click={handleInteraction}
        on:keydown={handleInteraction}
        role="textbox"
        tabindex="0">
        {@html htmlContent}
      </div>
      {#if attachments && attachments.length > 0}
        <div class="mt-2 space-y-2">
          {#each attachments as attachment}
            {#if attachment.attachment_type.startsWith("image")}
              <img
                src={attachment.data}
                alt={attachment.name}
                class="max-w-full max-h-[300px] object-contain rounded-xl cursor-pointer hover:opacity-90 transition-opacity"
              />
            {:else if attachment.attachment_type.startsWith("audio")}
              <div class="flex flex-col gap-2">
                <audio controls src={attachment.data} class="max-w-full">
                  Your browser does not support the audio element.
                </audio>
                {#if attachment.transcript}
                  <div class="text-sm text-muted-foreground">
                    Transcript: {attachment.transcript}
                  </div>
                {/if}
              </div>
            {:else if attachment.attachment_type.startsWith("text")}
              <div class="inline-flex items-center gap-2 bg-muted/50 px-3 py-2 rounded-lg group relative hover:bg-muted/70 transition-colors">
                <svg 
                  class="w-4 h-4 text-muted-foreground flex-none" 
                  xmlns="http://www.w3.org/2000/svg" 
                  viewBox="0 0 24 24" 
                  fill="none" 
                  stroke="currentColor" 
                  stroke-width="2" 
                  stroke-linecap="round" 
                  stroke-linejoin="round"
                >
                  <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                  <polyline points="14 2 14 8 20 8"/>
                </svg>

                <div class="max-w-[200px] overflow-hidden">
                  <p class="text-sm font-medium truncate leading-tight">{attachment.name}</p>
                  <p class="text-xs text-muted-foreground truncate leading-tight">
                    {getPreviewText(attachment.data)}
                  </p>
                </div>
              </div>
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
    background-color: rgb(24 24 27); /* zinc-900 */
    
    border-radius: 0.5rem;
    padding: 0.25rem 0.75rem 0.75rem;
    overflow-x: auto;
    margin: 0.5rem 0;
  }

  :global(.markdown-content pre code) {
    color: rgb(244 244 245); /* zinc-100 */
    font-size: 0.875rem;
    line-height: 1.5;
    white-space: pre-wrap;
    background-color: transparent;
    display: block;
    padding-top: 0.5rem;
  }

  /* Override Prism.js theme colors for better contrast */
  :global(.token.comment),
  :global(.token.prolog),
  :global(.token.doctype),
  :global(.token.cdata) {
    color: rgb(161 161 170); /* zinc-400 */
  }

  :global(.token.punctuation) {
    color: rgb(212 212 216); /* zinc-300 */
  }

  :global(.token.property),
  :global(.token.tag),
  :global(.token.boolean),
  :global(.token.number),
  :global(.token.constant),
  :global(.token.symbol) {
    color: rgb(167 139 250); /* purple-400 */
  }

  :global(.token.string) {
    color: rgb(134 239 172); /* green-300 */
  }

  :global(.token.operator),
  :global(.token.entity),
  :global(.token.url) {
    color: rgb(252 211 77); /* amber-300 */
  }

  :global(.token.keyword) {
    color: rgb(248 113 113); /* red-400 */
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
  background-color: rgb(24 24 27); /* zinc-900 */
  border: 1px solid rgb(63 63 70); /* zinc-700 */
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
    background: linear-gradient(
      to right,
      rgba(255, 255, 255, 0.1),
      transparent
    );
  }
</style>
