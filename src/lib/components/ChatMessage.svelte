<script lang="ts">
  export let type: 'sent' | 'received';
  export let content: string;
</script>

<div class={`mb-2 p-3 rounded-lg ${type === 'sent' ? 'bg-primary text-primary-foreground ml-auto' : 'bg-secondary text-secondary-foreground'} max-w-[80%]`}>
  <p>{content}</p>
</div>
