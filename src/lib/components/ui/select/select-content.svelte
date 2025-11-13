<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import { scale } from "svelte/transition";
    import { cn, flyAndScale } from "$lib/utils.js";
    import type { Snippet } from "svelte";

    type $$Props = SelectPrimitive.ContentProps & {
        children?: Snippet;
    };

    let {
        class: className,
        sideOffset = 4,
        inTransition = flyAndScale,
        inTransitionConfig = undefined,
        outTransition = scale,
        outTransitionConfig = {
            start: 0.95,
            opacity: 0,
            duration: 50,
        },
        children,
        ...restProps
    }: $$Props = $props();
</script>

<SelectPrimitive.Content
    {inTransition}
    {inTransitionConfig}
    {outTransition}
    {outTransitionConfig}
    {sideOffset}
    class={cn(
        "relative z-50 min-w-[8rem] overflow-hidden rounded-xl glass-panel-menu text-popover-foreground focus:outline-none",
        className,
    )}
    {...restProps}
>
    {@render children?.()}
</SelectPrimitive.Content>
