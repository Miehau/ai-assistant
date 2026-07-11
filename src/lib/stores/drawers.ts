import { writable } from "svelte/store";

export const settingsDrawerOpen = writable(false);
export const assistantsDrawerOpen = writable(false);
export const modelsDrawerOpen = writable(false);
export const usageDrawerOpen = writable(false);
export const mcpDrawerOpen = writable(false);
export const settingsSection = writable<"tools" | "backend" | "vault" | "connections">("tools");
