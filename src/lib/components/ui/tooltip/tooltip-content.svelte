<script lang="ts">
	import { Tooltip as TooltipPrimitive } from "bits-ui";
	import { cn, flyAndScale } from "$lib/utils.js";

	type $$Props = TooltipPrimitive.ContentProps;

	let className: $$Props["class"] = undefined;
	export let sideOffset: $$Props["sideOffset"] = 4;
	export let transition: $$Props["transition"] = flyAndScale;
	export let transitionConfig: $$Props["transitionConfig"] = {
		y: 8,
		duration: 150,
	};
	export { className as class };
</script>

<TooltipPrimitive.Content
	{transition}
	{transitionConfig}
	{sideOffset}
	class={cn(
		"z-50 overflow-hidden rounded-lg border border-white/10 bg-[#0f1012]/95 px-3 py-1.5 text-[11px] text-foreground/90 shadow-[0_12px_40px_rgba(0,0,0,0.45)]",
		className
	)}
	{...$$restProps}
>
	<slot />
</TooltipPrimitive.Content>
