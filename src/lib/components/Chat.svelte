<script lang="ts">
  import Share from "lucide-svelte/icons/share";
  import Paperclip from "lucide-svelte/icons/paperclip";
  import Mic from "lucide-svelte/icons/mic";
  import CornerDownLeft from "lucide-svelte/icons/corner-down-left";
  import { fly } from 'svelte/transition';

  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { writable } from 'svelte/store';
  import ChatMessage from './ChatMessage.svelte';

  let message = '';
  let correlationId = writable<string | null>(null);
  let messages: Array<{ type: 'sent' | 'received', content: string, intent?: string, slider?: string }> = [];

  async function sendEchoRequest() {
    console.log(`sending echo request with ${message}, correlation: ${$correlationId}`)
    const sentMessage = message;
    message = ''; // Clear the input immediately
    messages = [...messages, { type: 'sent', content: sentMessage }];
    const response = await fetch('http://localhost:3000/api/echo', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        message: sentMessage,
        correlationId: $correlationId,
      }),
    });

    if (response.ok) {
      const data: EchoResponse = await response.json();
      correlationId.set(data.correlationId || null);
      messages = [...messages, { 
        type: 'received', 
        content: data.message,
        intent: data.intent,
        slider: data.slider
      }];
      console.log(data);
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      sendEchoRequest();
    }
  }
</script>

<div
  class="relative flex h-full min-h-[50vh] flex-col rounded-xl bg-muted/50 p-4 lg:col-span-2"
>
  <Badge variant="outline" class="absolute right-3 top-3">Output</Badge>
  <div class="flex-1 overflow-y-auto mb-4 space-y-4">
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
  <form class="relative overflow-hidden rounded-lg border bg-background focus-within:ring-1 focus-within:ring-ring">
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
      <Button type="button" on:click={sendEchoRequest} size="sm" class="ml-auto gap-1.5">
        Send Message
        <CornerDownLeft class="size-3.5" />
      </Button>
    </div>
  </form>
</div>
