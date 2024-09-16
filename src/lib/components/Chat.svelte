<script lang="ts">
  import { onMount, afterUpdate } from "svelte";
  import { getConversations, sendChatMessage } from "$lib/services/api";
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

  type Message = {
    type: "sent" | "received";
    content: string;
  };

  let conversations = [];
  let currentConversationId: string | null = null;
  let chatContainer: HTMLElement | null = null;
  let currentMessage: string = "";
  let messages: Message[] = [];
  const models = [
    { value: "gpt-3.5-turbo", label: "GPT-3.5 Turbo" },
    { value: "gpt-4o-mini", label: "GPT-4o mini" },
  ];
  $: selectedModel = {
    value: models[0].value,
    label: models[0].label,
  };

  onMount(async () => {
    conversations = await getConversations();
  });

  afterUpdate(() => {
    scrollToBottom();
  });

  function scrollToBottom() {
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight; // Scroll to the bottom
    }
  }

  async function handleSendMessage() {
    const sentMessage = currentMessage;
    currentMessage = "";
    messages = [...messages, { type: "sent", content: sentMessage }];

    try {
      const response = await sendChatMessage(
        sentMessage,
        currentConversationId,
        selectedModel.value,
      );
      if (response && typeof response.text === "string") {
        messages = [...messages, { type: "received", content: response.text }];
        currentConversationId = response.conversationId; // Update currentConversationId with the returned value
        // scrollToBottom(); // Scroll after receiving a message
      } else {
        throw new Error("Invalid response format");
      }
    } catch (error) {
      console.error("Failed to send chat message:", error);
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault(); // Prevent default behavior
      handleSendMessage(); // Call the send message function
    }
  }
</script>

<div
  class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2"
>
  <div class="flex-1 overflow-hidden">
    <div
      bind:this={chatContainer}
      class="h-full overflow-y-auto pr-4 space-y-4"
    >
      {#each messages as msg}
        <div transition:fly={{ y: 20, duration: 300 }}>
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
      <Select.Root bind:selected={selectedModel}>
        <Select.Trigger class="w-[180px]">
          <Select.Value placeholder="Select a model" />
        </Select.Trigger>
        <Select.Content>
          {#each models as model}
            <Select.Item value={model.value}>{model.label}</Select.Item>
          {/each}
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
