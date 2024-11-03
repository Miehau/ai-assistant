<script lang="ts">
  import type { Attachment } from "$lib/types";
  
  export let type: "sent" | "received";
  export let content: string;
  export let attachments: (Attachment)[] | undefined = undefined;

  function getImageSrc(attachment: Attachment): string {
      return attachment.data;
  }
</script>

<div class="flex gap-3 {type === 'received' ? 'justify-start' : 'justify-end'}">
  <div class="rounded-lg px-3 py-2 max-w-[85%] {type === 'received' ? 'bg-muted' : 'bg-primary text-primary-foreground'}">
    {#if attachments && attachments.length > 0}
      <div class="flex flex-col gap-2 mb-2">
        {#each attachments as attachment}
          {#if attachment.attachment_type === 'image'}
            <img
              src={getImageSrc(attachment)}
              alt={attachment.name}
              class="max-w-full rounded-md"
            />
          {/if}
        {/each}
      </div>
    {/if}
    <p class="whitespace-pre-wrap">{content}</p>
  </div>
</div>

