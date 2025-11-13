<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import CaretSort from "svelte-radix/CaretSort.svelte";
	import { cn } from "$lib/utils.js";
	import type { Snippet } from "svelte";

	type $$Props = SelectPrimitive.TriggerProps & {
		children?: Snippet;
	};

	let { class: className, children, ...restProps }: $$Props = $props();
</script>

<SelectPrimitive.Trigger
	class={cn(
		"flex h-7 w-full items-center justify-between whitespace-nowrap rounded-xl glass-panel-minimal px-2 py-1 text-[11px] ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 aria-[invalid]:border-destructive [&>span]:line-clamp-1 data-[placeholder]:[&>span]:text-muted-foreground transition-all duration-200 hover:bg-white/[0.06]",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<div>
		<CaretSort class="h-4 w-4 opacity-50" />
	</div>
</SelectPrimitive.Trigger>
