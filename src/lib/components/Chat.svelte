<script lang="ts">
  import { onMount, afterUpdate } from "svelte";
  import { chatService } from "$lib/services/chat";
  import ChatMessage from "./ChatMessage.svelte";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Button } from "$lib/components/ui/button";
  import { Paperclip } from "lucide-svelte";
  import { Send } from "lucide-svelte";
  import * as Select from "$lib/components/ui/select";
  import type { Model } from "$lib/types/models";
  import { invoke } from "@tauri-apps/api/tauri";
  import type { Selected } from "bits-ui";
  import type { SystemPrompt } from "$lib/types";
  import { Image } from "lucide-svelte";
  import type { Message } from "$lib/types";
  import { conversationService } from "$lib/services/conversation";
  import { fade } from "svelte/transition";
  import { Square } from "lucide-svelte";
  import { Headphones } from 'lucide-svelte';

  let chatContainer: HTMLElement | null = null;
  let currentMessage: string = "";
  let messages: Message[] = [];
  let availableModels: Model[] = [];
  let systemPrompts: SystemPrompt[] = [];
  let selectedModel: Selected<string> = {
    value: "",
    label: "No models",
  };
  let selectedSystemPrompt: SystemPrompt | null = null;

  let lastScrollHeight = 0;
  let lastScrollTop = 0;

  let fileInput: HTMLInputElement;

  let attachments: FileAttachment[] = [];

  type FileAttachment = {
    attachment_type: string;
    name: string;
    data: string;
    position?: number;
    transcript?: string;
  };

  let unsubscribe: () => void;

  let autoScroll = true;
  let scrollTimeout: NodeJS.Timeout | null = null;

  let streamingEnabled = true;

  let isLoading = false;

  function toggleStreaming() {
    streamingEnabled = !streamingEnabled;
    chatService.setStreamResponse(streamingEnabled);
  }

  onMount(async () => {
    loadModels();
    loadSystemPrompts();

    // Initial load of messages if there's a current conversation
    const currentConversation = conversationService.getCurrentConversation();
    if (currentConversation) {
      const loadedMessages = await conversationService.getDisplayHistory(
        currentConversation.id,
      );
      messages = loadedMessages;
    }
  });

  onMount(() => {
    return () => {
      if (unsubscribe) unsubscribe();
    };
  });

  afterUpdate(() => {
    scrollToBottom();
  });

  function preserveScrollFromBottom() {
    if (chatContainer) {
      const newScrollHeight = chatContainer.scrollHeight;
      const visibleHeight = chatContainer.clientHeight;

      // Calculate distance from bottom before resize
      const distanceFromBottom =
        lastScrollHeight - (lastScrollTop + visibleHeight);

      // Restore the same distance from bottom after resize
      chatContainer.scrollTop =
        newScrollHeight - (distanceFromBottom + visibleHeight);

      // Update values for next resize
      lastScrollHeight = newScrollHeight;
      lastScrollTop = chatContainer.scrollTop;
    }
  }

  // Add resize observer
  onMount(() => {
    if (chatContainer) {
      const resizeObserver = new ResizeObserver(() => {
        preserveScrollFromBottom();
      });

      resizeObserver.observe(chatContainer);

      return () => {
        resizeObserver.disconnect();
      };
    }
  });

  function scrollToBottom() {
    if (chatContainer && autoScroll) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = newScrollTop;
    }
  }

  function handleScroll() {
    if (!chatContainer) return;
    
    // Clear existing timeout
    if (scrollTimeout) {
      clearTimeout(scrollTimeout);
    }

    const isScrolledToBottom = 
      Math.abs((chatContainer.scrollHeight - chatContainer.scrollTop) - chatContainer.clientHeight) < 10;
    
    // If user scrolls up, disable auto-scroll
    if (!isScrolledToBottom) {
      autoScroll = false;
    }

    // If user scrolls to bottom, enable auto-scroll after a delay
    if (isScrolledToBottom) {
      scrollTimeout = setTimeout(() => {
        autoScroll = true;
      }, 150); // 150ms delay before re-enabling auto-scroll
    }
  }

  // Add the scroll event listener in onMount
  onMount(() => {
    if (chatContainer) {
      chatContainer.addEventListener('scroll', handleScroll);
      return () => {
        chatContainer?.removeEventListener('scroll', handleScroll);
        if (scrollTimeout) clearTimeout(scrollTimeout);
      };
    }
  });

  function selectModel(v: Selected<string> | undefined) {
    if (v) {
      selectedModel = {
        value: v.value,
        label: `${v.value} • ${availableModels.find((m) => m.model_name === v.value)?.provider ?? ""}`,
      };
    }
  }

  async function handleSendMessage() {
    if (!currentMessage.trim() && attachments.length === 0) return;

    isLoading = true;
    
    try {
        // Create and display user message immediately
        const userMessage: Message = {
            type: "sent",
            content: currentMessage,
            attachments: attachments.length > 0 ? attachments : undefined,
        };
        messages = [...messages, userMessage];

        // Clear input fields
        const messageToSend = currentMessage;
        currentMessage = "";
        const attachmentsToSend = [...attachments];
        attachments = [];

        // Send message to AI (transcription happens inside handleSendMessage)
        await chatService.handleSendMessage(
            messageToSend,
            selectedModel.value,
            (chunk: string) => {
                if (!messages[messages.length - 1] || messages[messages.length - 1].type !== "received") {
                    messages = [...messages, { type: "received", content: chunk }];
                } else {
                    const updatedMessages = [...messages];
                    updatedMessages[updatedMessages.length - 1].content += chunk;
                    messages = updatedMessages;
                }
            },
            selectedSystemPrompt?.content || "You are a helpful assistant.",
            attachmentsToSend,
        );
    } catch (error) {
        console.error("Error sending message:", error);
    } finally {
        isLoading = false;
    }
  }

  async function loadModels() {
    try {
      const models = await invoke<Model[]>("get_models");
      availableModels = models.filter((model) => model.enabled);

      // Update selected model if we have available models
      if (availableModels.length > 0) {
        selectedModel = {
          value: availableModels[0].model_name,
          label: `${availableModels[0].model_name} • ${availableModels[0].provider}`,
        };
      }
    } catch (error) {
      console.error("Failed to load models:", error);
    }
  }

  async function loadSystemPrompts() {
    try {
      systemPrompts = await invoke("get_all_system_prompts");
      if (systemPrompts.length > 0 && !selectedSystemPrompt) {
        selectSystemPrompt(systemPrompts[0]);
      }
    } catch (error) {
      console.error("Failed to load system prompts:", error);
    }
  }

  function selectSystemPrompt(prompt: SystemPrompt) {
    selectedSystemPrompt = prompt;
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
    const file = input.files?.[0];

    if (file) {
        try {
            if (file.type.startsWith("image/")) {
                // Handle image files
                const base64 = await fileToBase64(file);
                const attachment: FileAttachment = {
                    attachment_type: file.type,
                    name: file.name,
                    data: base64,
                };
                attachments = [...attachments, attachment];
            } else if (file.type.startsWith("audio/")) {
                // Just save the audio file, don't transcribe yet
                const base64 = await fileToBase64(file);
                const attachment: FileAttachment = {
                    attachment_type: file.type,
                    name: file.name,
                    data: base64,
                };
                attachments = [...attachments, attachment];
            } else {
                // Handle text files
                const text = await file.text();
                currentMessage += text;
            }
        } catch (error) {
            console.error("Error reading file:", error);
        }
    }
    // Reset the input so the same file can be selected again
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

  function removeMessages() {
    const removeWithDelay = async () => {
      const delay = 100;
      while (messages.length > 0) {
        messages = messages.slice(0, -1);
        await new Promise(resolve => setTimeout(resolve, delay));
      }
      conversationService.setCurrentConversation(null);
    };
    removeWithDelay();
  }
</script>

<div
  class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2 w-full"
>
  <div class="flex-1 overflow-hidden">
    <div
      bind:this={chatContainer}
      class="h-full overflow-y-auto pr-4 space-y-4 w-full"
      on:scroll={handleScroll}
    >
      {#each messages as msg, i (i)}
        <div
          in:fade={{ duration: 200 }}
          out:fade={{ duration: 500 }}
          class="w-full message-container"
        >
          <ChatMessage
            type={msg.type}
            content={msg.content}
            attachments={msg.attachments}
          />
        </div>
      {/each}
    </div>
  </div>

  <form
    class="mt-4 relative overflow-hidden rounded-lg border bg-background focus-within:ring-1 focus-within:ring-ring"
  >
    <input
      type="file"
      accept=".txt,.md,.json,.js,.ts,.py,.rs,.svelte,image/*,audio/*"
      bind:this={fileInput}
      style="display: none;"
      on:change={handleFileChange}
    />
    <Label for="message" class="sr-only">Message</Label>
    <Textarea
      id="message"
      bind:value={currentMessage}
      on:keydown={handleKeydown}
      placeholder="Type your message here..."
      class="min-h-12 resize-none border-0 p-3 shadow-none focus-visible:ring-0"
    />
    {#if attachments.length > 0}
      <div class="flex flex-wrap gap-2 px-3 pb-2">
        {#each attachments as attachment, index}
          {#if attachment.attachment_type.startsWith("image")}
            <div
              class="flex items-center gap-2 bg-muted px-2 py-1 rounded-md group relative"
            >
              <Image class="size-4" />
              <span class="text-sm">{attachment.name}</span>
              <button
                class="ml-1 text-muted-foreground hover:text-destructive transition-colors"
                on:click={() => {
                  attachments = attachments.filter((_, i) => i !== index);
                }}
                type="button"
                aria-label="Remove attachment"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="14"
                  height="14"
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
          {:else if attachment.attachment_type.startsWith("audio")}
            <div class="flex items-center gap-2 bg-muted px-2 py-1 rounded-md group relative">
              <Headphones class="size-4" />
              <span class="text-sm">{attachment.name}</span>
              <audio class="h-6 w-24 mx-1" controls src={attachment.data}>
                Your browser does not support the audio element.
              </audio>
              <button
                class="ml-1 text-muted-foreground hover:text-destructive transition-colors"
                on:click={() => {
                  attachments = attachments.filter((_, i) => i !== index);
                }}
                type="button"
                aria-label="Remove attachment"
              >
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="14"
                  height="14"
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
          {/if}
        {/each}
      </div>
    {/if}
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
        <Tooltip.Content side="top">Upload File (Text or Image)</Tooltip.Content
        >
      </Tooltip.Root>
      <Tooltip.Root>
        <Tooltip.Trigger asChild let:builder>
          <Button
            builders={[builder]}
            variant="ghost"
            size="icon"
            on:click={() => {
              removeMessages();
            }}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M5 12h14" />
              <path d="M12 5v14" />
            </svg>
            <span class="sr-only">New Conversation</span>
          </Button>
        </Tooltip.Trigger>
        <Tooltip.Content side="top">Start New Conversation</Tooltip.Content>
      </Tooltip.Root>
      <!-- <div class="flex items-center p-3 pb-0"> -->
      <Select.Root
        selected={{ value: selectedSystemPrompt?.id ?? "", label: selectedSystemPrompt?.name ?? "" }}
        onSelectedChange={(v) => {
          if (v) {
            const prompt = systemPrompts.find((p) => p.id === v.value);
            if (prompt) selectSystemPrompt(prompt);
          }
        }}
      >
        <Select.Trigger class="min-w-[180px] w-fit mr-2">
          <Select.Value placeholder="Select system prompt">
            {#if selectedSystemPrompt}
              <div class="flex items-center gap-2">
                <span class="truncate max-w-[150px]">{selectedSystemPrompt.name}</span>
              </div>
            {/if}
          </Select.Value>
        </Select.Trigger>
        <Select.Content>
          {#if systemPrompts.length === 0}
            <Select.Item value="" disabled
              >No system prompts available</Select.Item
            >
          {:else}
            {#each systemPrompts as prompt}
              <Select.Item value={prompt.id}>
                <div class="flex items-center gap-2">
                  <span>{prompt.name}</span>
                </div>
              </Select.Item>
            {/each}
          {/if}
        </Select.Content>
      </Select.Root>

      <Select.Root onSelectedChange={selectModel} selected={selectedModel}>
        <Select.Trigger class="min-w-[180px] w-fit">
          <Select.Value placeholder="Select a model">
            {#if selectedModel}
              <div class="flex items-center gap-2">
                <span>{selectedModel.value}</span>
                {#if selectedModel.label}
                  <span class="text-sm text-muted-foreground">•</span>
                  <span class="text-sm text-muted-foreground">
                    {selectedModel.label.split(" • ")[1] ?? ""}
                  </span>
                {/if}
              </div>
            {/if}
          </Select.Value>
        </Select.Trigger>
        <Select.Content>
          {#if availableModels.length === 0}
            <Select.Item value="" disabled>No models available</Select.Item>
          {:else}
            {#each availableModels as model}
              <Select.Item value={model.model_name}>
                <div class="flex items-center gap-2 whitespace-nowrap">
                  <span>{model.model_name}</span>
                  <span class="text-sm text-muted-foreground">•</span>
                  <span class="text-sm text-muted-foreground"
                    >{model.provider}</span
                  >
                </div>
              </Select.Item>
            {/each}
          {/if}
        </Select.Content>
      </Select.Root>
      <!-- </div> -->
      <Tooltip.Root>
        <Tooltip.Trigger asChild let:builder>
          <Button
            builders={[builder]}
            variant="ghost"
            size="icon"
            type="button"
            on:click={toggleStreaming}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              class={streamingEnabled ? "text-primary" : "text-muted-foreground"}
            >
              <path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242" />
              <path d="M12 12v9" />
              <path d="m8 17 4 4 4-4" />
            </svg>
            <span class="sr-only">Toggle Streaming</span>
          </Button>
        </Tooltip.Trigger>
        <Tooltip.Content side="top">
          {streamingEnabled ? 'Disable' : 'Enable'} Streaming
        </Tooltip.Content>
      </Tooltip.Root>
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
</div>

<style>
  /* Add to existing styles */
  :global(.message-container) {
    transform-origin: center;
    perspective: 1000px;
  }

  :global(.message-container:out) {
    animation: dustAway 0.3s ease-out forwards;
  }

  @keyframes dustAway {
    0% {
      opacity: 1;
      transform: translateX(0) rotate(0);
    }
    100% {
      opacity: 0;
      transform: translateX(100px) rotate(10deg);
    }
  }
</style>
