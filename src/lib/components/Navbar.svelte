<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { onMount, tick } from "svelte";
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
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import ConversationDrawer from "$lib/components/conversation/ConversationDrawer.svelte";
  import BranchDrawer from "$lib/components/branch/BranchDrawer.svelte";
  import SettingsDrawer from "$lib/components/SettingsDrawer.svelte";
  import { currentConversation } from "$lib/services/conversation";
  import { settingsDrawerOpen, assistantsDrawerOpen, modelsDrawerOpen, usageDrawerOpen } from "$lib/stores/drawers";
  import AssistantsDrawer from "$lib/components/AssistantsDrawer.svelte";
  import ModelsDrawer from "$lib/components/ModelsDrawer.svelte";
  import UsageDrawer from "$lib/components/UsageDrawer.svelte";

  $: currentPath = $page.url.pathname;
  $: hasConversation = Boolean($currentConversation?.id);

  let isConversationDrawerOpen = false;
  let isBranchDrawerOpen = false;
  let isNavOpen = false;
  let skipNavTransition = false;
  $: isAnyDrawerOpen =
    isConversationDrawerOpen || isBranchDrawerOpen || $settingsDrawerOpen ||
    $assistantsDrawerOpen || $modelsDrawerOpen || $usageDrawerOpen;

  function toggleNav() {
    const next = !isNavOpen;
    isNavOpen = next;
    if (next) {
      isConversationDrawerOpen = false;
      $settingsDrawerOpen = false;
      isBranchDrawerOpen = false;
      $assistantsDrawerOpen = false;
      $modelsDrawerOpen = false;
      $usageDrawerOpen = false;
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
    $settingsDrawerOpen = !$settingsDrawerOpen;
    closeNav();
  }

  function toggleAssistantsDrawer() {
    $assistantsDrawerOpen = !$assistantsDrawerOpen;
    closeNav();
  }

  function toggleModelsDrawer() {
    $modelsDrawerOpen = !$modelsDrawerOpen;
    closeNav();
  }

  function toggleUsageDrawer() {
    $usageDrawerOpen = !$usageDrawerOpen;
    closeNav();
  }

  async function openNavAfterClose() {
    skipNavTransition = true;
    isNavOpen = true;
    await tick();
    requestAnimationFrame(() => {
      skipNavTransition = false;
    });
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

  function handleNavigate(event: MouseEvent, path: string) {
    if (event.defaultPrevented) return;
    if (event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;

    event.preventDefault();
    closeNav();
    if (path === currentPath) return;
    void goto(path);
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
    class="fixed inset-0 z-30 bg-black/45 nav-overlay"
    class:nav-overlay-open={isNavOpen}
    class:nav-no-transition={skipNavTransition}
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
    class:nav-no-transition={skipNavTransition}
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
              onclick={(event) => handleNavigate(event, "/")}
            >
              <SquareTerminal class="size-4" />
              <span>Chat</span>
            </a>
            <button
              type="button"
              class={navItemClasses($assistantsDrawerOpen)}
              onclick={toggleAssistantsDrawer}
            >
              <Users class="size-4" />
              <span>Assistants</span>
            </button>
            <button
              type="button"
              class={navItemClasses($modelsDrawerOpen)}
              onclick={toggleModelsDrawer}
            >
              <CodeXML class="size-4" />
              <span>Models</span>
            </button>
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
            <button
              type="button"
              class={navItemClasses($usageDrawerOpen)}
              onclick={toggleUsageDrawer}
            >
              <TrendingUp class="size-4" />
              <span>Usage</span>
            </button>
          </div>
        </div>

        <div class="space-y-2">
          <p class="nav-section-label">System</p>
          <div class="grid gap-1">
            <button
              type="button"
              class={navItemClasses($settingsDrawerOpen)}
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

<ConversationDrawer bind:isOpen={isConversationDrawerOpen} onBack={openNavAfterClose} />
<SettingsDrawer bind:isOpen={$settingsDrawerOpen} onBack={openNavAfterClose} />
<AssistantsDrawer bind:isOpen={$assistantsDrawerOpen} onBack={openNavAfterClose} />
<ModelsDrawer bind:isOpen={$modelsDrawerOpen} onBack={openNavAfterClose} />
<UsageDrawer bind:isOpen={$usageDrawerOpen} onBack={openNavAfterClose} />
{#if $currentConversation?.id}
  <BranchDrawer
    conversationId={$currentConversation.id}
    open={isBranchDrawerOpen}
    onClose={handleBranchDrawerClose}
  />
{/if}
