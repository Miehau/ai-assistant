<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Button } from "$lib/components/ui/button";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Paperclip, Send, Square } from "lucide-svelte";
  import { chatService } from "$lib/services/chat";

  export let currentMessage: string = "";
  export let attachments: FileAttachment[] = [];
  export let isLoading: boolean = false;

  type FileAttachment = {
    attachment_type: "audio" | "image" | "text/plain" | string;
    name: string;
    data: string;
    transcript?: string;
  };

  let fileInput: HTMLInputElement;
  const dispatch = createEventDispatcher();

  function handleSendMessage() {
    if (!currentMessage.trim() && attachments.length === 0) return;
    dispatch('sendMessage', { message: currentMessage, attachments });
  }

  async function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  async function handleFileChange(event: Event) {
    const input = event.target as HTMLInputElement;
    const files = Array.from(input.files || []);

    if (files.length > 0) {
        try {
            const newAttachments = await Promise.all(files.map(async (file) => {
                if (file.type.startsWith('text/') || file.name.match(/\.(txt|md|json|js|ts|py|rs|svelte)$/)) {
                    // For text files, read as text directly
                    const text = await file.text();
                    return {
                        attachment_type: "text/plain" as const,
                        name: file.name,
                        data: text
                    };
                } else if (file.type.startsWith('audio/')) {
                    // For audio files
                    const base64 = await fileToBase64(file);
                    return {
                        attachment_type: "audio" as const,
                        name: file.name,
                        data: base64
                    };
                } else {
                    // For images and other files, use base64
                    const base64 = await fileToBase64(file);
                    return {
                        attachment_type: "image" as const,
                        name: file.name,
                        data: base64
                    };
                }
            }));

            attachments = [...attachments, ...newAttachments];
        } catch (error) {
            console.error("Error reading files:", error);
        }
    }
    // Reset the input so the same files can be selected again
    input.value = "";
  }

  function handleFileUpload() {
    fileInput?.click();
  }

  function handleKeydown(event: KeyboardEvent) {
    // Send message on Enter (but not with Shift+Enter)
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSendMessage();
    }
  }
</script>

<form
  class="relative overflow-hidden rounded-lg border bg-background focus-within:ring-1 focus-within:ring-ring"
>
  <input
    type="file"
    multiple
    accept=".txt,.md,.json,.js,.ts,.py,.rs,.svelte,image/*,audio/*,text/*"
    bind:this={fileInput}
    style="display: none;"
    on:change={handleFileChange}
  />
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-2 px-3 pb-2">
      {#each attachments as attachment, index}
        <Tooltip.Root>
          <Tooltip.Trigger asChild>
            <div class="square-attachment">
              {#if attachment.attachment_type === "image"}
                <div class="square-attachment-thumbnail">
                  <img 
                    src={attachment.data} 
                    alt={attachment.name} 
                    class="square-attachment-image" 
                  />
                </div>
              {:else}
                <div class="square-attachment-icon-container">
                  {#if attachment.attachment_type === "audio"}
                    <svg 
                      class="square-attachment-icon"
                      xmlns="http://www.w3.org/2000/svg" 
                      viewBox="0 0 24 24" 
                      fill="none" 
                      stroke="currentColor" 
                      stroke-width="2" 
                      stroke-linecap="round" 
                      stroke-linejoin="round"
                    >
                      <path d="M18 8a6 6 0 0 0-12 0v12h12V8z"></path>
                      <path d="M6 20a2 2 0 0 0 4 0"></path>
                      <path d="M14 20a2 2 0 0 0 4 0"></path>
                    </svg>
                  {:else if attachment.attachment_type === "text/plain" || attachment.attachment_type.startsWith("text/")}
                    <svg 
                      class="square-attachment-icon" 
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
                  {/if}
                </div>
              {/if}
              <div class="square-attachment-name">
                {attachment.name.length > 10 ? attachment.name.slice(0, 8) + '...' : attachment.name}
              </div>
              <button
                class="square-attachment-remove"
                on:click={() => {
                  attachments = attachments.filter((_, i) => i !== index);
                }}
                type="button"
                aria-label="Remove attachment"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="10"
                  height="10"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            </div>
          </Tooltip.Trigger>
          <Tooltip.Content side="top">
            {attachment.name}
          </Tooltip.Content>
        </Tooltip.Root>
      {/each}
    </div>
  {/if}
  <Label for="message" class="sr-only">Message</Label>
  <Textarea
    id="message"
    bind:value={currentMessage}
    on:keydown={handleKeydown}
    placeholder="Type your message here..."
    class="min-h-12 resize-none border-0 p-3 shadow-none focus-visible:ring-0"
  />
  <div class="flex items-center p-3 pt-0">
    <Tooltip.Root>
      <Tooltip.Trigger asChild let:builder>
        <Button
          builders={[builder]}
          variant="ghost"
          size="icon"
          type="button"
          on:click={handleFileUpload}
        >
          <Paperclip class="size-4" />
          <span class="sr-only">Upload File</span>
        </Button>
      </Tooltip.Trigger>
      <Tooltip.Content side="top">Upload File (Text or Image)</Tooltip.Content>
    </Tooltip.Root>
    
    <slot name="controls"></slot>
    
    <Button
      type="button"
      on:click={isLoading ? () => chatService.cancelCurrentRequest() : handleSendMessage}
      size="sm"
      class="ml-auto gap-1.5"
      variant={isLoading ? "destructive" : "default"}
    >
      {#if isLoading}
        <Square class="size-3.5" />
      {:else}
        <Send class="size-3.5" />
      {/if}
    </Button>
  </div>
</form>
