<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import Check from "svelte-radix/Check.svelte";
	import { cn } from "$lib/utils.js";
	import type { Snippet } from "svelte";

	type $$Props = SelectPrimitive.ItemProps & {
		children?: Snippet<[{ selected: boolean }]>;
	};

	let { value, label = undefined, disabled = undefined, class: className, children: contentSnippet, ...restProps }: $$Props = $props();
</script>

<SelectPrimitive.Item
	{value}
	{disabled}
	{label}
	class={cn(
		"relative flex w-full cursor-default select-none items-center rounded-lg py-1.5 pl-2 pr-8 text-sm outline-none data-[disabled]:pointer-events-none data-[highlighted]:bg-white/10 data-[highlighted]:text-foreground data-[disabled]:opacity-50 transition-colors duration-50 [contain:layout_style]",
		className
	)}
	{...restProps}
>
	{#snippet children({ selected })}
		<span class="absolute right-2 flex h-3.5 w-3.5 items-center justify-center">
			{#if selected}
				<Check class="h-4 w-4" />
			{/if}
		</span>
		{@render contentSnippet?.({ selected })}
		{#if !contentSnippet}
			{label || value}
		{/if}
	{/snippet}
</SelectPrimitive.Item>
