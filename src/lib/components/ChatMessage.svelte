<script lang="ts">
  export let type: 'sent' | 'received';
  export let content: string;
  export let intent: string | undefined = undefined;
  export let slider: string | undefined = undefined;
</script>

<div class={`mb-2 p-3 rounded-lg ${type === 'sent' ? 'bg-primary text-primary-foreground ml-auto' : 'bg-secondary text-secondary-foreground'} max-w-[80%]`}>
  <p>{content}</p>
  {#if intent}
    <p class="text-sm mt-1"><strong>Intent:</strong> {intent}</p>
  {/if}
  {#if slider}
    <p class="text-sm"><strong>Slider:</strong> {slider}</p>
  {/if}
</div>
