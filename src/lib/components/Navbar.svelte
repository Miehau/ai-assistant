<script lang="ts">
  import Triangle from "lucide-svelte/icons/triangle";
  import Bot from "lucide-svelte/icons/bot";
  import SquareTerminal from "lucide-svelte/icons/square-terminal";
  import CodeXML from "lucide-svelte/icons/code-xml";
  import Settings2 from "lucide-svelte/icons/settings-2";
  import LifeBuoy from "lucide-svelte/icons/life-buoy";
  import Book from "lucide-svelte/icons/book";
  import SquareUser from "lucide-svelte/icons/square-user";
  import Users from "lucide-svelte/icons/users";
  import History from "lucide-svelte/icons/history";
  import TrendingUp from "lucide-svelte/icons/trending-up";
  import Network from "lucide-svelte/icons/network";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Tooltip from "$lib/components/ui/tooltip/index.js";
  import { page } from "$app/stores";
  import ConversationDrawer from "$lib/components/conversation/ConversationDrawer.svelte";
  import BranchDrawer from "$lib/components/branch/BranchDrawer.svelte";
  import { currentConversation } from "$lib/services/conversation";

  $: currentPath = $page.url.pathname;

  let isConversationDrawerOpen = false;
  let isBranchDrawerOpen = false;

  function toggleConversationDrawer() {
    isConversationDrawerOpen = !isConversationDrawerOpen;
  }

  function toggleBranchDrawer() {
    isBranchDrawerOpen = !isBranchDrawerOpen;
  }

  function handleBranchDrawerClose() {
    isBranchDrawerOpen = false;
  }
</script>

<Tooltip.Provider>
  <aside class="fixed left-0 top-0 bottom-0 z-20 flex flex-col glass-panel border-r-0 pt-8">
    <div class="border-b border-white/10 p-2">
      <Button variant="outline" size="icon" aria-label="Home" class="hover-glow-green">
        <Triangle class="size-5 fill-foreground" />
      </Button>
    </div>
    <nav class="grid gap-1 p-2">
      <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <a href="/" {...props}>
            <Button
              variant="ghost"
              size="icon"
              class="rounded-lg transition-all {currentPath === '/' ? 'glass-light glow-green' : 'hover:glass-badge'}"
              aria-label="Playground"
            >
              <SquareTerminal class="size-5" />
            </Button>
          </a>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Playground</Tooltip.Content>
    </Tooltip.Root>
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <a href="/models" {...props}>
            <Button
              variant="ghost"
              size="icon"
              class="rounded-lg transition-all {currentPath === '/models' ? 'glass-light glow-green' : 'hover:glass-badge'}"
              aria-label="Models"
            >
              <CodeXML class="size-5" />
            </Button>
          </a>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>API</Tooltip.Content>
    </Tooltip.Root>
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <a href="/assistants" {...props}>
            <Button
              variant="ghost"
              size="icon"
              class="rounded-lg transition-all {currentPath === '/assistants' ? 'glass-light glow-green' : 'hover:glass-badge'}"
              aria-label="Assistants"
            >
              <Users class="size-5" />
            </Button>
          </a>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Assistants</Tooltip.Content>
    </Tooltip.Root>
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon"
            class="rounded-lg transition-all {isConversationDrawerOpen ? 'glass-light glow-green' : 'hover:glass-badge'}"
            aria-label="Conversation History"
            onclick={toggleConversationDrawer}
          >
            <History class="size-5" />
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Conversation History</Tooltip.Content>
    </Tooltip.Root>
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <a href="/usage" {...props}>
            <Button
              variant="ghost"
              size="icon"
              class="rounded-lg transition-all {currentPath === '/usage' ? 'glass-light glow-cyan' : 'hover:glass-badge'}"
              aria-label="Usage Statistics"
            >
              <TrendingUp class="size-5" />
            </Button>
          </a>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Usage Statistics</Tooltip.Content>
    </Tooltip.Root>
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon"
            class="rounded-lg transition-all {isBranchDrawerOpen ? 'glass-light glow-purple' : 'hover:glass-badge'}"
            aria-label="Branch Tree"
            onclick={toggleBranchDrawer}
          >
            <Network class="size-5" />
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Branch Tree</Tooltip.Content>
    </Tooltip.Root>
  </nav>
  <nav class="mt-auto grid gap-1 p-2">
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon"
            class="mt-auto rounded-lg transition-all hover:glass-badge"
            aria-label="Help"
          >
            <LifeBuoy class="size-5" />
          </Button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content side="right" sideOffset={5}>Help</Tooltip.Content>
    </Tooltip.Root>
  </nav>
</aside>
</Tooltip.Provider>

<ConversationDrawer bind:isOpen={isConversationDrawerOpen} />
{#if $currentConversation?.id}
  <BranchDrawer
    conversationId={$currentConversation.id}
    open={isBranchDrawerOpen}
    onClose={handleBranchDrawerClose}
  />
{/if}
