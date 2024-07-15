<script lang="ts">
  import Share from "lucide-svelte/icons/share";
  import Paperclip from "lucide-svelte/icons/paperclip";
  import Mic from "lucide-svelte/icons/mic";
  import CornerDownLeft from "lucide-svelte/icons/corner-down-left";
  import { fly } from 'svelte/transition';
  import { onMount, afterUpdate, setContext } from 'svelte';

  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { writable } from 'svelte/store';
  import ChatMessage from './ChatMessage.svelte';
  import { sendEchoRequest } from '$lib/services/api';

  let message = '';
  let correlationId = writable<string | null>(null);
  let messages: Array<{ type: 'sent' | 'received', content: string, intent?: string, slider?: string }> = [];
  let chatContainer: HTMLElement;
  let currentSlider = writable<string | null>(null);

  setContext('currentSlider', currentSlider);

  function scrollToBottom() {
    if (chatContainer) {
      chatContainer.scrollTop = chatContainer.scrollHeight;
    }
  }

  onMount(() => {
    scrollToBottom();
  });

  afterUpdate(() => {
    scrollToBottom();
  });

  async function handleSendEchoRequest() {
    console.log(`sending echo request with ${message}, correlation: ${$correlationId}`)
    const sentMessage = message;
    message = ''; // Clear the input immediately
    messages = [...messages, { type: 'sent', content: sentMessage }];
    
    try {
      const data = await sendEchoRequest(sentMessage, $correlationId);
      correlationId.set(data.correlationId || null);
      messages = [...messages, { 
        type: 'received', 
        content: data.message,
        intent: data.intent,
        slider: data.slider
      }];
      currentSlider.set(data.slider || null);
      console.log(data);
    } catch (error) {
      console.error('Failed to send echo request:', error);
      // Handle error (e.g., show an error message to the user)
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      handleSendEchoRequest();
    }
  }
</script>

<div class="relative flex flex-col h-full min-h-[50vh] rounded-xl bg-muted/50 p-4 lg:col-span-2">
  <Badge variant="outline" class="absolute right-3 top-3">Output</Badge>
  
  <div class="flex-1 overflow-hidden">
    <div bind:this={chatContainer} class="h-full overflow-y-auto pr-4 space-y-4">
      {#each messages as msg}
        <div transition:fly="{{ y: 20, duration: 300 }}">
          <ChatMessage 
            type={msg.type} 
            content={msg.content} 
            intent={msg.intent} 
            slider={msg.slider}
          />
        </div>
      {/each}
    </div>
  </div>
  
  <form class="mt-4 relative overflow-hidden rounded-lg border bg-background focus-within:ring-1 focus-within:ring-ring">
    <Label for="message" class="sr-only">Message</Label>
    <Textarea
      id="message"
      bind:value={message}
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
      <Button type="button" on:click={handleSendEchoRequest} size="sm" class="ml-auto gap-1.5">
        Send Message
        <CornerDownLeft class="size-3.5" />
      </Button>
    </div>
  </form>
</div>
