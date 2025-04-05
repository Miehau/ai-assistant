<!-- Type definitions are now provided by @types/prismjs -->
<!-- No need for a module script anymore -->

<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import Prism from "prismjs";
  import "prismjs/themes/prism-tomorrow.css";
  import "prismjs/components/prism-javascript";
  import "prismjs/components/prism-json";
  import "prismjs/components/prism-typescript";
  import "prismjs/components/prism-python";
  import "prismjs/components/prism-bash";
  import "prismjs/components/prism-markdown";
  import "prismjs/components/prism-java";
  import "prismjs/components/prism-kotlin";
  import "prismjs/components/prism-rust";
  import "prismjs/components/prism-sql";
  import "prismjs/components/prism-mermaid";
  import "prismjs/components/prism-typescript";
  import "prismjs/components/prism-git";
  import "prismjs/components/prism-docker";
  import "prismjs/components/prism-csv";
  import type { Attachment } from "$lib/types";
  import { fileService } from "$lib/services/fileService";
  import { onDestroy } from "svelte";

  export let type: "sent" | "received";
  export let content: string;
  export let attachments: Attachment[] | undefined = undefined;

  // Track loading states for attachments
  let loadingStates: Record<string, boolean> = {};
  let loadedImages: Record<string, string> = {};
  let loadedThumbnails: Record<string, string> = {};

  // Image modal state
  let imageModalOpen = false;
  let currentImageSrc = "";
  let currentImageAlt = "";

  // Load file data from the backend if needed
  async function loadFileData(attachment: Attachment, index: number) {
    // If we already have the data or no file path, nothing to do
    if (attachment.data || !attachment.file_path) return;

    const key = `${index}-${attachment.name}`;
    loadingStates[key] = true;

    try {
      // Load the file data from the backend
      const data = await fileService.getFile(attachment.file_path, true);
      attachment.data = data;
    } catch (error) {
      console.error(`Error loading file ${attachment.name}:`, error);
    } finally {
      loadingStates[key] = false;
    }
  }

  // Load image thumbnail for better performance
  async function loadImageThumbnail(attachment: Attachment, index: number) {
    if (!attachment.file_path || !attachment.file_metadata?.thumbnail_path)
      return;

    const key = `${index}-${attachment.name}`;

    try {
      // Load the thumbnail from the backend
      const thumbnailData = await fileService.getImageThumbnail(
        attachment.file_metadata.thumbnail_path,
      );
      loadedThumbnails[key] = thumbnailData;
    } catch (error) {
      console.error(`Error loading thumbnail for ${attachment.name}:`, error);
    }
  }

  // Progressive image loading - first show thumbnail, then load full image
  async function loadFullImage(attachment: Attachment, index: number) {
    if (!attachment.file_path) return;

    const key = `${index}-${attachment.name}`;
    loadingStates[key] = true;

    try {
      // Load the full image
      const imageData = await fileService.getFile(attachment.file_path, true);
      loadedImages[key] = imageData;
    } catch (error) {
      console.error(`Error loading full image for ${attachment.name}:`, error);
    } finally {
      loadingStates[key] = false;
    }
  }

  // Initialize attachment loading
  onMount(() => {
    if (attachments) {
      attachments.forEach((attachment, index) => {
        if (attachment.attachment_type === "image" && attachment.file_path) {
          // For images, load thumbnails first
          loadImageThumbnail(attachment, index);
        } else {
          // For other types, load the full data
          loadFileData(attachment, index);
        }
      });
    }
  });

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
      const highlightedCode = language
        ? Prism.highlight(
            code,
            Prism.languages[language] || Prism.languages.text,
            language,
          )
        : escapeHtml(code);

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
    renderer.link = function ({ href, title, text }) {
      const titleAttr = title ? ` title="${title}"` : "";
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
    if (
      event instanceof KeyboardEvent &&
      event.key !== "Enter" &&
      event.key !== " "
    ) {
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
    return words.slice(0, 15).join(" ") + " ...";
  }

  function getAttachmentUrl(attachment: Attachment): string {
    return attachment.data || attachment.file_path || "";
  }

  function openImageModal(url: string, name: string) {
    currentImageSrc = url;
    currentImageAlt = name;
    imageModalOpen = true;
  }

  function closeImageModal() {
    imageModalOpen = false;
    currentImageSrc = "";
    currentImageAlt = "";
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
        tabindex="0"
      >
        {@html htmlContent}
      </div>
      {#if attachments && attachments.length > 0}
        <div class="mt-2 space-y-2">
          {#each attachments as attachment}
            <!-- Import FileAttachment component for each attachment -->
            {#await import('./files/FileAttachment.svelte') then FileAttachment}
              <svelte:component 
                this={FileAttachment.default} 
                {attachment} 
                showVersioning={true} 
              />
            {/await}
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<!-- Image Modal -->
{#if imageModalOpen}
  <div 
    class="fixed inset-0 bg-black/80 flex items-center justify-center z-50" 
    role="dialog" 
    aria-modal="true"
    tabindex="-1"
  >
    <!-- Add a transparent button that covers the entire modal for keyboard accessibility -->
    <button 
      class="absolute inset-0 w-full h-full opacity-0" 
      on:click={closeImageModal} 
      on:keydown={(e) => e.key === 'Escape' && closeImageModal()}
      aria-label="Close modal background"
    ></button>
    <div class="relative max-w-[90vw] max-h-[90vh] overflow-auto">
      <button 
        type="button" 
        class="absolute top-2 right-2 bg-black/50 text-white rounded-full p-1 hover:bg-black/70 transition-colors"
        on:click|stopPropagation={closeImageModal}
        aria-label="Close image preview"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 6 6 18" />
          <path d="m6 6 12 12" />
        </svg>
      </button>
      <img 
        src={currentImageSrc} 
        alt={currentImageAlt} 
        class="max-w-full max-h-[90vh] object-contain"
      />
      <div class="absolute bottom-2 left-0 right-0 text-center text-white bg-black/50 py-1 px-2">
        {currentImageAlt}
      </div>
    </div>
  </div>
{/if}

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
