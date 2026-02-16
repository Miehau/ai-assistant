<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { onMount } from "svelte";
  import SquareTerminal from "lucide-svelte/icons/square-terminal";
  import CodeXML from "lucide-svelte/icons/code-xml";
  import Settings2 from "lucide-svelte/icons/settings-2";
  import Users from "lucide-svelte/icons/users";
  import History from "lucide-svelte/icons/history";
  import TrendingUp from "lucide-svelte/icons/trending-up";
  import Network from "lucide-svelte/icons/network";
  import PanelLeftOpen from "lucide-svelte/icons/panel-left-open";
  import PanelLeftClose from "lucide-svelte/icons/panel-left-close";
  import { Button } from "$lib/components/ui/button/index.js";
  import { page } from "$app/stores";
  import ConversationDrawer from "$lib/components/conversation/ConversationDrawer.svelte";
  import BranchDrawer from "$lib/components/branch/BranchDrawer.svelte";
  import SettingsDrawer from "$lib/components/SettingsDrawer.svelte";
  import { currentConversation } from "$lib/services/conversation";

  $: currentPath = $page.url.pathname;
  $: hasConversation = Boolean($currentConversation?.id);

  let isConversationDrawerOpen = false;
  let isBranchDrawerOpen = false;
  let isSettingsDrawerOpen = false;
  let isNavOpen = false;
  $: isAnyDrawerOpen = isConversationDrawerOpen || isBranchDrawerOpen || isSettingsDrawerOpen;

  function toggleNav() {
    const next = !isNavOpen;
    isNavOpen = next;
    if (next) {
      isConversationDrawerOpen = false;
      isSettingsDrawerOpen = false;
      isBranchDrawerOpen = false;
    }
  }

  function closeNav() {
    isNavOpen = false;
  }

  function toggleConversationDrawer() {
    isConversationDrawerOpen = !isConversationDrawerOpen;
    closeNav();
  }

  function toggleBranchDrawer() {
    if (!hasConversation) return;
    isBranchDrawerOpen = !isBranchDrawerOpen;
    closeNav();
  }

  function toggleSettingsDrawer() {
    isSettingsDrawerOpen = !isSettingsDrawerOpen;
    closeNav();
  }

  function handleBranchDrawerClose() {
    isBranchDrawerOpen = false;
  }

  function navItemClasses(active: boolean, disabled = false) {
    return [
      "nav-item",
      active ? "nav-item-active" : "",
      disabled ? "nav-item-disabled" : ""
    ]
      .filter(Boolean)
      .join(" ");
  }

  function shouldIgnoreShortcut(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
    return target.isContentEditable;
  }

  onMount(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (isAnyDrawerOpen) return;
      if (shouldIgnoreShortcut(event.target)) return;

      const key = event.key.toLowerCase();
      if ((event.metaKey || event.ctrlKey) && key === "b" && !event.shiftKey && !event.altKey) {
        event.preventDefault();
        toggleNav();
      }
    };

    window.addEventListener("keydown", handler);
    return () => {
      window.removeEventListener("keydown", handler);
    };
  });
</script>

{#if !isNavOpen && !isAnyDrawerOpen}
  <div class="fixed left-3 top-11 z-30">
    <Button
      variant="ghost"
      size="icon"
      class="nav-toggle rounded-xl"
      aria-label="Open navigation"
      onclick={toggleNav}
    >
      <PanelLeftOpen class="size-5" />
    </Button>
  </div>
{/if}

  <div
    class="fixed inset-0 z-30 bg-black/45 backdrop-blur-sm nav-overlay"
    class:nav-overlay-open={isNavOpen}
    onclick={closeNav}
    role="button"
    tabindex={isNavOpen ? 0 : -1}
    aria-hidden={!isNavOpen}
    onkeydown={(e) => {
      if (!isNavOpen) return;
      if (e.key === "Escape") closeNav();
    }}
  ></div>
  <aside
    class="fixed left-0 top-8 bottom-0 z-40 w-[260px] nav-drawer nav-panel rounded-r-2xl overflow-hidden"
    class:nav-panel-open={isNavOpen}
    aria-hidden={!isNavOpen}
    onclick={(event) => event.stopPropagation()}
  >
    <div class="flex h-full flex-col">
      <div class="flex items-center justify-between border-b border-white/10 px-4 py-4">
        <div class="flex items-center gap-2">
          <span class="text-[11px] uppercase tracking-wide text-muted-foreground/70">Navigation</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          class="rounded-lg"
          aria-label="Close navigation"
          onclick={closeNav}
        >
          <PanelLeftClose class="size-5" />
        </Button>
      </div>

      <div class="flex-1 overflow-y-auto px-3 py-4 space-y-5">
        <div class="space-y-2">
          <p class="nav-section-label">Core</p>
          <div class="grid gap-1">
            <a
              href="/"
              class={navItemClasses(currentPath === "/")}
              aria-current={currentPath === "/" ? "page" : undefined}
              onclick={closeNav}
            >
              <SquareTerminal class="size-4" />
              <span>Chat</span>
            </a>
            <a
              href="/assistants"
              class={navItemClasses(currentPath === "/assistants")}
              aria-current={currentPath === "/assistants" ? "page" : undefined}
              onclick={closeNav}
            >
              <Users class="size-4" />
              <span>Assistants</span>
            </a>
            <a
              href="/models"
              class={navItemClasses(currentPath === "/models")}
              aria-current={currentPath === "/models" ? "page" : undefined}
              onclick={closeNav}
            >
              <CodeXML class="size-4" />
              <span>Models</span>
            </a>
          </div>
        </div>

        <div class="space-y-2">
          <p class="nav-section-label">Context</p>
          <div class="grid gap-1">
            <button
              type="button"
              class={navItemClasses(isConversationDrawerOpen)}
              onclick={toggleConversationDrawer}
            >
              <History class="size-4" />
              <span>Conversation History</span>
            </button>
            <button
              type="button"
              class={navItemClasses(isBranchDrawerOpen, !hasConversation)}
              onclick={toggleBranchDrawer}
              aria-disabled={!hasConversation}
            >
              <Network class="size-4" />
              <span>Branch Tree</span>
            </button>
          </div>
        </div>

        <div class="space-y-2">
          <p class="nav-section-label">Insights</p>
          <div class="grid gap-1">
            <a
              href="/usage"
              class={navItemClasses(currentPath === "/usage")}
              aria-current={currentPath === "/usage" ? "page" : undefined}
              onclick={closeNav}
            >
              <TrendingUp class="size-4" />
              <span>Usage</span>
            </a>
          </div>
        </div>

        <div class="space-y-2">
          <p class="nav-section-label">System</p>
          <div class="grid gap-1">
            <button
              type="button"
              class={navItemClasses(isSettingsDrawerOpen)}
              onclick={toggleSettingsDrawer}
            >
              <Settings2 class="size-4" />
              <span>Settings</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </aside>

<ConversationDrawer bind:isOpen={isConversationDrawerOpen} />
<SettingsDrawer bind:isOpen={isSettingsDrawerOpen} />
{#if $currentConversation?.id}
  <BranchDrawer
    conversationId={$currentConversation.id}
    open={isBranchDrawerOpen}
    onClose={handleBranchDrawerClose}
  />
{/if}
