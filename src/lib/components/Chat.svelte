<script lang="ts">
  import { onMount, afterUpdate } from "svelte";
  import { sendChatMessage } from "$lib/services/chat";
  import { fly } from "svelte/transition";
  import ChatMessage from "./ChatMessage.svelte";
  import { Label } from "$lib/components/ui/label";
  import { Textarea } from "$lib/components/ui/textarea";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Button } from "$lib/components/ui/button";
  import { Paperclip } from "lucide-svelte";
  import { Mic } from "lucide-svelte";
  import { Send } from "lucide-svelte";
  import * as Select from "$lib/components/ui/select";
  import type { Model } from "$lib/types/models";
  import { invoke } from "@tauri-apps/api/tauri";
    import type { Selected } from "bits-ui";

  type Message = {
    type: "sent" | "received";
    content: string;
  };

  let currentConversationId: string | null = null;
  let chatContainer: HTMLElement | null = null;
  let currentMessage: string = "";
  let currentStreamedMessage: Message | null = {
    type: "received",
    content: "",
  };
  let messages: Message[] = [];
  let availableModels: Model[] = [];
  $: selectedModel = {
    value: availableModels[0]?.model_name ?? "",
    label: `${availableModels[0]?.model_name ?? "No models"} • ${availableModels[0]?.provider ?? ""}`
  };
  $: streamResponse = true;

  let lastScrollHeight = 0;
  let lastScrollTop = 0;  // Added this declaration

  afterUpdate(() => {
    scrollToBottom();
  });

  function preserveScrollFromBottom() {
    if (chatContainer) {
      const newScrollHeight = chatContainer.scrollHeight;
      const visibleHeight = chatContainer.clientHeight;
      
      // Calculate distance from bottom before resize
      const distanceFromBottom = lastScrollHeight - (lastScrollTop + visibleHeight);
      
      // Restore the same distance from bottom after resize
      chatContainer.scrollTop = newScrollHeight - (distanceFromBottom + visibleHeight);
      
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
    if (chatContainer) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = newScrollTop;
    }
  }
  
  function selectModel(v: Selected<{ value: string; label: string }>) {
    console.log(v.value);
    selectedModel = {
      value: v.value,
      label: v.label
    };
  }

  async function handleSendMessage() {
    const sentMessage = currentMessage;
    currentMessage = "";
    messages = [...messages, { type: "sent", content: sentMessage }];

    let isFirstChunk = true;

    let onStreamResponse = (chunk: string) => {
      if (isFirstChunk) {
        // Add a new "received" message when we get the first chunk
        messages = [...messages, { type: "received", content: chunk }];
        isFirstChunk = false;
      } else {
        // Update the last message with the new chunk
        messages[messages.length - 1].content += chunk;
      }
      messages = [...messages]; // Trigger Svelte reactivity
    };

    try {
      const response = await sendChatMessage(
        sentMessage,
        currentConversationId,
        selectedModel.value,
        streamResponse,
        onStreamResponse,
      );
      if (!streamResponse && response && typeof response.text === "string") {
        messages[messages.length - 1] = {
          type: "received",
          content: response.text,
        };
      }
      currentConversationId = response.conversationId;
      currentStreamedMessage = null;
      if (streamResponse) {
        handleStreamResponse(response.text);
      } else if (response && typeof response.text === "string") {
        messages = [...messages, { type: "received", content: response.text }];
        currentConversationId = response.conversationId;
      } else {
        throw new Error("Invalid response format");
      }
    } catch (error) {
      console.error("Failed to send chat message:", error);
    }
  }

  function handleStreamResponse(chunk: string) {
    // messages = [...messages, { type: "received", content: chunk }];
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault(); // Prevent default behavior
      handleSendMessage(); // Call the send message function
    }
  }

  async function loadModels() {
    try {
      const models = await invoke<Model[]>("get_models");
      // Only show enabled models
      availableModels = models.filter(model => model.enabled);
      
      // Update selected model if we have available models
      if (availableModels.length > 0) {
        selectedModel = {
          value: availableModels[0].model_name,
          label: `${availableModels[0].model_name} • ${availableModels[0].provider}`
        };
      }
    } catch (error) {
      console.error("Failed to load models:", error);
    }
  }

  onMount(() => {
    loadModels();
  });
</script>

<div
  class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2 w-full"
>
  <div class="flex-1 overflow-hidden">
    <div
      bind:this={chatContainer}
      class="h-full overflow-y-auto pr-4 space-y-4 w-full"
    >
      {#each messages as msg}
        <div transition:fly={{ y: 20, duration: 300 }} class="w-full">
          <ChatMessage type={msg.type} content={msg.content} />
        </div>
      {/each}
    </div>
  </div>

  <form
    class="mt-4 relative overflow-hidden rounded-lg border bg-background focus-within:ring-1 focus-within:ring-ring"
  >
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
          <Button builders={[builder]} variant="ghost" size="icon">
            <Paperclip class="size-4" />
            <span class="sr-only">Attach file</span>
          </Button>
        </Tooltip.Trigger>
        <Tooltip.Content side="top">Attach File</Tooltip.Content>
      </Tooltip.Root>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <Button variant="ghost" size="icon">
            <Mic class="size-4" />
            <span class="sr-only">Use Microphone</span>
          </Button>
        </Tooltip.Trigger>
        <Tooltip.Content side="top">Use Microphone</Tooltip.Content>
      </Tooltip.Root>
      <!-- <div class="flex items-center p-3 pb-0"> -->
      <Select.Root onSelectedChange={selectModel} selected={selectedModel}>
        <Select.Trigger class="min-w-[180px] w-fit">
          <Select.Value placeholder="Select a model">
            {#if selectedModel}
              <div class="flex items-center gap-2">
                <span>{selectedModel.value}</span>
                <span class="text-sm text-muted-foreground">•</span>
                <span class="text-sm text-muted-foreground">{selectedModel.label.split(' • ')[1]}</span>
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
                  <span class="text-sm text-muted-foreground">{model.provider}</span>
                </div>
              </Select.Item>
            {/each}
          {/if}
        </Select.Content>
      </Select.Root>
      <!-- </div> -->
      <Button
        type="button"
        on:click={handleSendMessage}
        size="sm"
        class="ml-auto gap-1.5"
      >
        <Send class="size-3.5" />
      </Button>
    </div>
  </form>
</div>
