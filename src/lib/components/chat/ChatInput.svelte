<script lang="ts">
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Button } from "$lib/components/ui/button";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Paperclip, Send, Square } from "lucide-svelte";
  import { chatService } from "$lib/services/chat";
  import { fileService } from "$lib/services/fileService";
  import type { Attachment, FileMetadata, Message } from "$lib/types";
  import { get } from "svelte/store";
  import { currentConversation } from "$lib/services/conversation";
  import { open } from '@tauri-apps/api/dialog';
  import CostEstimator from "./CostEstimator.svelte";
  import type { Snippet } from "svelte";

  interface Props {
    currentMessage?: string;
    attachments?: Attachment[];
    isLoading?: boolean;
    modelId?: string;
    messages?: Message[];
    systemPrompt?: string;
    onSendMessage?: (data: { message: string; attachments: Attachment[] }) => void;
    controls?: Snippet;
  }

  let {
    currentMessage = $bindable(""),
    attachments = $bindable([]),
    isLoading = false,
    modelId = "",
    messages = [],
    systemPrompt = "",
    onSendMessage,
    controls
  }: Props = $props();
  
  // Generate a temporary message ID for file uploads
  // This will be replaced with the actual message ID when the message is saved
  const tempMessageId = crypto.randomUUID();
  
  // Track upload progress
  let uploading = $state(false);
  let uploadProgress = $state<Record<string, number>>({});
  
  // Drag and drop state
  let dragActive = $state(false);

  let fileInput: HTMLInputElement;

  function handleSendMessage() {
    if (!currentMessage.trim() && attachments.length === 0) return;
    onSendMessage?.({ message: currentMessage, attachments });
  }

  async function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  // Common function to handle files from both input change and drag-drop
  async function handleFiles(files: File[]) {
    const conversationData = get(currentConversation);
    
    // If no conversation is selected, create a fallback conversation ID
    const conversationId = conversationData?.id || "temp-conversation-" + Date.now();

    if (files.length > 0) {
      // Set uploading state to true
      uploading = true;
      
      try {
        // Initialize progress for each file
        files.forEach(file => {
          uploadProgress[file.name] = 0;
        });
        
        const newAttachments = await Promise.all(files.map(async (file, index) => {
          // Simulate progress updates (in a real implementation, you would get this from the upload API)
          const progressInterval = setInterval(() => {
            if (uploadProgress[file.name] < 90) {
              uploadProgress[file.name] += 5;
              uploadProgress = {...uploadProgress};
            }
          }, 100);
          
          try {
            // Determine the attachment type based on file MIME type
            let attachmentType = "";
            if (file.type.startsWith('text/') || file.name.match(/\.(txt|md|json|js|ts|py|rs|svelte)$/)) {
              attachmentType = "text/plain";
            } else if (file.type.startsWith('audio/')) {
              attachmentType = "audio";
            } else if (file.type.startsWith('image/')) {
              attachmentType = "image";
            } else {
              attachmentType = file.type || "application/octet-stream";
            }
            
            // Update progress
            uploadProgress[file.name] = 50;
            uploadProgress = {...uploadProgress};
            
            // Get the file path using Tauri's native dialog API
            const filePath = await open({
              multiple: false,
              directory: false
            });
            
            if (!filePath || typeof filePath !== 'string') {
              throw new Error('No file path selected');
            }

            // Let Rust handle the file operations directly from the temp path
            const result = await fileService.uploadFileFromPath(
              filePath,
              file.name,
              file.type || "application/octet-stream",
              conversationId,
              tempMessageId
            );
            
            // Update progress
            uploadProgress[file.name] = 95;
            uploadProgress = {...uploadProgress};
            
            // Complete progress
            uploadProgress[file.name] = 100;
            uploadProgress = {...uploadProgress};
            
            // Clear interval
            clearInterval(progressInterval);
            
            // Create an attachment with file metadata
            const attachment: Attachment = {
              name: file.name,
              attachment_type: attachmentType.startsWith('image/') ? 'image' :
                              attachmentType.startsWith('audio/') ? 'audio' :
                              'text',
              file_path: result.metadata.path,
              mime_type: file.type || "application/octet-stream",
              file_metadata: result.metadata as FileMetadata
            };
            
            return attachment;
          } catch (error) {
            console.error("Error uploading file:", error);
            
            // Clear interval and mark as failed
            clearInterval(progressInterval);
            uploadProgress[file.name] = -1; // Use -1 to indicate failure
            uploadProgress = {...uploadProgress};
            
            // Fallback to the old approach if upload fails
            console.log('Falling back to direct file reading approach');
            if (file.type.startsWith('text/') || file.name.match(/\.(txt|md|json|js|ts|py|rs|svelte)$/)) {
              const text = await file.text();
              return {
                attachment_type: "text" as const, // Use "text" instead of "text/plain"
                name: file.name,
                data: text
              };
            } else {
              // Need to get base64 data for fallback
              const fallbackBase64 = await fileToBase64(file);
              if (file.type.startsWith('audio/')) {
                return {
                  attachment_type: "audio" as const,
                  name: file.name,
                  data: fallbackBase64
                };
              } else {
                return {
                  attachment_type: "image" as const, // Default to image for other types
                  name: file.name,
                  data: fallbackBase64
                };
              }
            }
          }
        }));

        attachments = [...attachments, ...newAttachments];
      } catch (error) {
        console.error("Error processing files:", error);
      } finally {
        // Reset uploading state
        uploading = false;
        uploadProgress = {};
      }
    }
    
    // Reset the file input
    if (fileInput) {
      fileInput.value = "";
    }
  }
  
  // Handler for file input change
  async function handleFileChange(event: Event) {
    const input = event.target as HTMLInputElement;
    const files = Array.from(input.files || []);
    await handleFiles(files);
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

<Tooltip.Provider>
<style>
  .square-attachment {
    position: relative;
    width: 80px;
    height: 80px;
    border-radius: 8px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.05);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    transition: all 0.3s ease;
  }

  .square-attachment:hover {
    box-shadow: 0 0 20px rgba(82, 183, 136, 0.3);
    transform: scale(1.05);
  }

  .square-attachment-thumbnail {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .square-attachment-image {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .square-attachment-icon-container {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }

  .square-attachment-icon {
    width: 100%;
    height: 100%;
    color: hsl(var(--muted-foreground));
  }

  .square-attachment-name {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background-color: rgba(0, 0, 0, 0.5);
    color: white;
    font-size: 0.7rem;
    padding: 2px 4px;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .square-attachment-remove {
    position: absolute;
    top: 2px;
    right: 2px;
    background-color: rgba(0, 0, 0, 0.5);
    color: white;
    border-radius: 50%;
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    cursor: pointer;
  }

  .square-attachment-remove:hover {
    background-color: rgba(255, 0, 0, 0.7);
  }
  
  /* Drag and drop styles */
  .drag-active::before {
    content: '';
    position: absolute;
    inset: -4px;
    background-color: hsl(var(--primary) / 0.1);
    border: 2px dashed hsl(var(--primary));
    border-radius: 0.5rem;
    z-index: 10;
    pointer-events: none;
  }
</style>

<form
  class="relative overflow-hidden rounded-2xl chat-input-panel focus-within:ring-1 focus-within:ring-white/20 transition-all duration-200 mx-6 mb-6"
  class:drag-active={dragActive}
  ondragenter={(e) => {
    e.preventDefault();
    e.stopPropagation();
    dragActive = true;
  }}
  ondragover={(e) => {
    e.preventDefault();
    e.stopPropagation();
    dragActive = true;
  }}
  ondragleave={(e) => {
    e.preventDefault();
    e.stopPropagation();
    // Simple implementation to avoid TypeScript errors
    dragActive = false;
  }}
  ondrop={(e) => {
    e.preventDefault();
    e.stopPropagation();
    dragActive = false;

    if (e.dataTransfer?.files.length) {
      const files = Array.from(e.dataTransfer.files);
      handleFiles(files);
    }
  }}
>
  <input
    type="file"
    multiple
    accept=".txt,.md,.json,.js,.ts,.py,.rs,.svelte,image/*,audio/*,text/*"
    bind:this={fileInput}
    style="display: none;"
    onchange={handleFileChange}
  />
  {#if uploading}
    <div class="px-3 pb-2">
      {#each Object.entries(uploadProgress) as [fileName, progress]}
        <div class="mb-2">
          <div class="flex justify-between text-xs mb-1">
            <span class="truncate max-w-[200px]">{fileName}</span>
            <span>
              {#if progress === -1}
                <span class="text-destructive">Failed</span>
              {:else}
                {progress}%
              {/if}
            </span>
          </div>
          <div class="h-1 w-full bg-muted rounded-full overflow-hidden">
            <div
              class="h-full {progress === -1 ? 'bg-destructive' : 'gradient-primary'} transition-all duration-300"
              style="width: {progress === -1 ? '100' : progress}%"
            ></div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
  
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-2 px-3 pb-2">
      {#each attachments as attachment, index}
        <Tooltip.Root>
          <Tooltip.Trigger asChild>
            {#snippet child({ props })}
            <div class="square-attachment" {...props}>
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
                  {:else if attachment.attachment_type === "text" || attachment.attachment_type === "text/plain"}
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
                onclick={() => {
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
            {/snippet}
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
    onkeydown={handleKeydown}
    placeholder="Type your message here..."
    class="min-h-11 resize-none border-0 px-3 py-2 text-sm leading-6 shadow-none focus-visible:ring-0 bg-transparent"
  />
  <div class="flex flex-col">
    <!-- Top row: Action buttons and send -->
    <div class="flex items-center justify-between px-3 pt-1 pb-2">
      <div class="flex items-center gap-1.5">
        <Tooltip.Root>
          <Tooltip.Trigger asChild>
            {#snippet child({ props })}
            <Button
              {...props}
              variant="ghost"
              size="icon"
              type="button"
              onclick={handleFileUpload}
            >
              <Paperclip class="size-4" />
              <span class="sr-only">Upload File</span>
            </Button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="top">Upload File (Text or Image)</Tooltip.Content>
        </Tooltip.Root>
      </div>

      <Button
        type="button"
        size="icon"
        variant={isLoading ? "destructive" : "ghost"}
        onclick={isLoading ? () => chatService.cancelCurrentRequest() : handleSendMessage}
      >
        {#if isLoading}
          <Square class="size-4" />
        {:else}
          <Send class="size-4" />
        {/if}
      </Button>
    </div>

    <!-- Bottom row: Model controls and token info -->
    <div class="px-3 pb-3">
      {#if controls}
        {@render controls()}
      {/if}
    </div>
  </div>
</form>
</Tooltip.Provider>
