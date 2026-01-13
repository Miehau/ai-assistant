<!-- Type definitions are now provided by @types/prismjs -->
<!-- No need for a module script anymore -->

<script lang="ts">
  import { marked } from "marked";
  import { onMount } from "svelte";
  import Prism from "prismjs";
  import "prismjs/themes/prism-tomorrow.css";

  // Lazy-load Prism languages on demand using autoloader
  import "prismjs/plugins/autoloader/prism-autoloader";

  // Pre-load only the most common languages for instant availability
  import "prismjs/components/prism-javascript";
  import "prismjs/components/prism-typescript";
  import "prismjs/components/prism-python";

  // Configure autoloader to load other languages on demand
  // This reduces initial bundle size while maintaining full language support
  if (typeof Prism !== 'undefined' && Prism.plugins && Prism.plugins.autoloader) {
    Prism.plugins.autoloader.languages_path =
      'https://cdnjs.cloudflare.com/ajax/libs/prism/1.30.0/components/';
  }
  import type { Attachment, ToolCallRecord } from "$lib/types";
  import { fileService } from "$lib/services/fileService";
  import { onDestroy } from "svelte";
  import { getCachedParse, setCachedParse } from "$lib/utils/markdownCache";

  export let type: "sent" | "received";
  export let content: string;
  export let attachments: Attachment[] | undefined = undefined;
  export let model: string | undefined = undefined;
  export let messageId: string | undefined = undefined;
  export let conversationId: string | undefined = undefined;
  export let isStreaming: boolean = false;
  export let tool_calls: ToolCallRecord[] | undefined = undefined;

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

  // Progressive rendering: show raw text immediately, parse in background
  let htmlContent = '';
  let parseTimeout: number | null = null;
  let lastContent = '';

  // Parse markdown with caching to prevent redundant parsing on remount
  function parseMarkdown(text: string) {
    // Check cache first
    const cached = getCachedParse(text);
    if (cached) {
      return cached;
    }

    try {
      const result = marked(text);
      setCachedParse(text, result);
      return result;
    } catch (error) {
      console.error('Markdown parsing error:', error);
      return escapeHtml(text);
    }
  }

  // Progressive rendering during rapid updates
  $: {
    if (!isStreaming && content !== lastContent) {
      lastContent = content;

      // Clear existing timeout
      if (parseTimeout !== null) {
        clearTimeout(parseTimeout);
      }

      // Parse with minimal delay (16ms = 1 frame) for smooth streaming
      parseTimeout = window.setTimeout(() => {
        htmlContent = parseMarkdown(content);
        parseTimeout = null;
      }, 16) as unknown as number;
    } else if (isStreaming) {
      lastContent = content;
      htmlContent = '';
    }
  }

  onMount(async () => {
    const renderer = new marked.Renderer();

    renderer.code = ({ text, lang }: RendererCode) => {
      const code = text || "";
      const language = lang || "text";

      // Prism autoloader will load languages on demand
      // If language isn't loaded yet, fall back to plain text
      let highlightedCode: string;
      try {
        if (language && Prism.languages[language]) {
          highlightedCode = Prism.highlight(code, Prism.languages[language], language);
        } else {
          // Use plain text or escaped HTML if language not available
          highlightedCode = escapeHtml(code);
        }
      } catch (error) {
        console.warn(`Failed to highlight ${language} code:`, error);
        highlightedCode = escapeHtml(code);
      }

      return `
        <div class="code-block-wrapper relative group mb-4">
          <div class="code-block-header">
            <span class="code-language-label">${language}</span>
          </div>
          <button
            class="copy-button opacity-0 group-hover:opacity-100 absolute top-1 right-2
            p-1.5 rounded-md hover:bg-white/10 transition-all duration-200"
            data-copy="${encodeURIComponent(code)}"
          >
            <svg class="w-3.5 h-3.5 text-white/90" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
          </button>
          <pre class="code-block-glass"><code class="language-${language}">${highlightedCode}</code></pre>
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

    if (!isStreaming) {
      // Defer initial parse to idle time to avoid blocking the main thread
      // This allows the component to mount quickly and parse during idle time
      const parseWhenIdle = () => {
        htmlContent = parseMarkdown(content);
      };

      if ('requestIdleCallback' in window) {
        requestIdleCallback(parseWhenIdle);
      } else {
        // Fallback for browsers without requestIdleCallback support
        setTimeout(parseWhenIdle, 0);
      }
    }
  });

  onDestroy(() => {
    // Clean up timeout on component destruction
    if (parseTimeout !== null) {
      clearTimeout(parseTimeout);
    }
  });

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

  function formatToolPayload(payload: unknown): string {
    if (payload === undefined) return "";
    try {
      return JSON.stringify(payload, null, 2);
    } catch {
      return String(payload);
    }
  }

  function toolStatusLabel(call: ToolCallRecord): string {
    if (call.success === true) return "success";
    if (call.success === false) return "failed";
    return "running";
  }
</script>

<div
  class="flex gap-3 {type === 'received' ? 'justify-start' : 'justify-end'}"
  data-message-id={messageId}
  data-conversation-id={conversationId}
>
  <div
    class="rounded-2xl px-4 py-1.5 w-full max-w-5xl {type === 'received'
      ? 'message-glass-ai'
      : 'text-primary-foreground message-glass-user'}"
  >
    <!-- Message header with model -->
    {#if type === 'sent' && model}
      <div class="text-[10px] text-primary-foreground/50 text-right mb-1">{model}</div>
    {/if}
    <div class="prose prose-sm dark:prose-invert max-w-none">
      <div
        class="markdown-content"
        onclick={handleInteraction}
        onkeydown={handleInteraction}
        role="textbox"
        tabindex="0"
      >
        {#if htmlContent}
          {@html htmlContent}
        {:else}
          <div style="white-space: pre-wrap;">{content}</div>
        {/if}
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

      {#if type === 'received' && tool_calls && tool_calls.length > 0}
        <div class="mt-3 border-t border-border/40 pt-3">
          <details class="rounded-xl border border-border/40 bg-background/40 px-3 py-2">
            <summary class="cursor-pointer list-none">
              <div class="flex items-center justify-between gap-2">
                <span class="text-xs font-semibold text-foreground">
                  Tool calls
                </span>
                <span class="text-[10px] uppercase tracking-wide text-muted-foreground">
                  {tool_calls.length}
                </span>
              </div>
            </summary>

            <div class="mt-3 space-y-3">
              {#each tool_calls as call (call.execution_id)}
                <div class="rounded-lg border border-border/40 bg-muted/30 p-3">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <p class="text-xs font-semibold text-foreground">{call.tool_name}</p>
                      <p class="text-[10px] text-muted-foreground">
                        {toolStatusLabel(call)}
                      </p>
                    </div>
                    {#if call.duration_ms !== undefined}
                      <span class="text-[10px] text-muted-foreground">
                        {call.duration_ms} ms
                      </span>
                    {/if}
                  </div>

                  <div class="mt-2 grid gap-2 md:grid-cols-2">
                    <div>
                      <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">Args</p>
                      <pre class="max-h-40 overflow-auto rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.args)}
                      </pre>
                    </div>
                    <div>
                      <p class="text-[10px] uppercase tracking-wide text-muted-foreground mb-1">
                        {call.success === false ? "Error" : "Result"}
                      </p>
                      <pre class="max-h-40 overflow-auto rounded-md bg-background/60 p-2 text-[11px] font-mono text-foreground">
{formatToolPayload(call.success === false ? call.error : call.result)}
                      </pre>
                    </div>
                  </div>
                </div>
              {/each}
            </div>
          </details>
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
      onclick={closeImageModal}
      onkeydown={(e) => e.key === 'Escape' && closeImageModal()}
      aria-label="Close modal background"
    ></button>
    <div class="relative max-w-[90vw] max-h-[90vh] overflow-auto">
      <button
        type="button"
        class="absolute top-2 right-2 bg-black/50 text-white rounded-full p-1 hover:bg-black/70 transition-colors"
        onclick={(e) => { e.stopPropagation(); closeImageModal(); }}
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

    border-bottom-left-radius: 0.5rem;
    border-bottom-right-radius: 0.5rem;
    padding: 0.5rem 0.75rem 0.75rem;
    overflow-x: auto;
    margin: 0;
  }

  :global(.markdown-content pre code) {
    color: rgb(244 244 245); /* zinc-100 */
    font-size: 0.875rem;
    line-height: 1.5;
    white-space: pre-wrap;
    background-color: transparent;
    display: block;
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
    transition: all 300ms ease;
  }

  :global(.code-block-header) {
    background: linear-gradient(135deg, rgba(139, 92, 246, 0.3), rgba(59, 130, 246, 0.3)); /* purple to blue gradient */
    padding: 0.375rem 0.75rem 0.25rem;
    border-top-left-radius: 0.5rem;
    border-top-right-radius: 0.5rem;
    margin-bottom: 0;
  }

  :global(.code-language-label) {
    color: rgb(212 212 216); /* zinc-300 - more visible */
    font-size: 0.6875rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  :global(.code-block-wrapper .copy-button) {
    opacity: 1;
  }

  /* Ensure glass effects work within message bubbles */
  :global(.message-glass-ai .code-block-wrapper),
  :global(.message-glass-user .code-block-wrapper) {
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(20px);
  }
</style>
