<script lang="ts">
  import Settings from "lucide-svelte/icons/settings";
  import Share from "lucide-svelte/icons/share";

  import Chat from "$lib/components/Chat.svelte";
  import MainLayout from "$lib/components/MainLayout.svelte";
  import SettingsComponent from "$lib/components/Settings.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Drawer from "$lib/components/ui/drawer/index.js";
</script>

<MainLayout>
  <div class="grid h-screen w-full">
  
  <div class="flex flex-col">
    <header
      class="sticky top-0 z-10 flex h-[57px] items-center gap-1 border-b bg-background px-4"
    >
      <h1 class="text-xl font-semibold">Playground</h1>
      <Drawer.Root>
        <Drawer.Trigger asChild let:builder>
          <Button builders={[builder]} variant="ghost" size="icon" class="md:hidden">
            <Settings class="size-4" />
            <span class="sr-only">Settings</span>
          </Button>
        </Drawer.Trigger>
        <Drawer.Content class="max-h-[80vh]">
          <Drawer.Header>
            <Drawer.Title>Configuration</Drawer.Title>
            <Drawer.Description>
              Configure the settings for the model and messages.
            </Drawer.Description>
          </Drawer.Header>
          <div class="overflow-auto p-4 pt-0">
            <SettingsComponent />
          </div>
        </Drawer.Content>
      </Drawer.Root>
      <Button variant="outline" size="sm" class="ml-auto gap-1.5 text-sm">
        <Share class="size-3.5" />
        Share
      </Button>
    </header>
    <main class="grid flex-1 gap-4 overflow-auto p-4 md:grid-cols-2 lg:grid-cols-3">
      <div class="relative hidden flex-col items-start gap-8 md:flex">
        <SettingsComponent />
      </div>
      <Chat />
    </main>
  </div>
  </div>
</MainLayout>
