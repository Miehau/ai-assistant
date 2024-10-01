<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import * as Dialog from "$lib/components/ui/dialog";

  export let isOpen: boolean;
  export let onClose: () => void;
  export let onSubmit: (formData: {
    title: string,
    image: string | null,
    ingredientsText: string,
    methodStepsText: string,
    tags: string
  }) => void;
  export let recipe: Recipe | null = null;

  let title = "";
  let imagePreview: string | null = null;
  let ingredientsText = "";
  let methodStepsText = "";
  let tags = "";

  $: isReadOnly = !!recipe;
  $: modalTitle = isReadOnly ? recipe?.title : 'Add New Recipe';

  function reset() {
    title = "";
    imagePreview = null;
    ingredientsText = "";
    methodStepsText = "";
    tags = "";
  }

  $: if (recipe) {
    title = recipe.title;
    imagePreview = recipe.image;
    ingredientsText = recipe.ingredients.map(ing => `${ing.name}, ${ing.amount}`).join('\n');
    methodStepsText = recipe.methodSteps.map(step => step.description).join('\n\n');
    tags = recipe.tags.join(', ');
  }

  function handleClose() {
    onClose();
    // Reset after a short delay to avoid visual glitches
    setTimeout(reset, 100);
  }
</script>

<Dialog.Root open={isOpen} onOpenChange={handleClose}>
  <Dialog.Content class="sm:max-w-[90vw] sm:w-[900px] max-h-[90vh] flex flex-col">
    <Dialog.Header>
      <Dialog.Title>{modalTitle}</Dialog.Title>
    </Dialog.Header>
    <div class="flex-grow overflow-y-auto">
      <div class="flex h-[70vh]">
        <!-- Left column: Image and Title -->
        <div class="w-1/3 pr-4 flex flex-col relative">
        <div class="bottom-[20%] left-0 right-4 bg-opacity-90 p-2 rounded-lg mb-4">
            <div class="mb-2">
                <label for="title" class="block mb-1">Title</label>
                <Input type="text" id="title" bind:value={title} readonly={isReadOnly} />
            </div>
            <div>
                <label for="tags" class="block mb-1">Tags</label>
                <Input type="text" id="tags" bind:value={tags} readonly={isReadOnly} />
            </div>
        </div>
          <div class="mb-4 h-[60%] flex items-center justify-center overflow-hidden">
            {#if imagePreview}
              <img src={imagePreview} alt="Recipe preview" class="max-w-[100%] h-auto object-center object-cover" />
            {:else}
              <div class="w-full h-full bg-gray-200 flex items-center justify-center">
                No image available
              </div>
            {/if}
          </div>
    
        </div>
        
        <!-- Right column: Ingredients and Method Steps -->
        <div class="w-2/3 flex">
          <div class="w-1/2 pr-2">
            <label for="ingredients" class="block mb-1">Ingredients</label>
            <Textarea
              id="ingredients"
              bind:value={ingredientsText}
              readonly={isReadOnly}
              class="resize-none h-full"
            />
          </div>
          <div class="w-1/2 pl-2">
            <label for="methodSteps" class="block mb-1">Method Steps</label>
            <Textarea
              id="methodSteps"
              bind:value={methodStepsText}
              readonly={isReadOnly}
              class="resize-none h-full"
            />
          </div>
        </div>
      </div>
    </div>
    <Dialog.Footer class="sm:mt-4">
      <Button on:click={onClose}>Close</Button>
      {#if !isReadOnly}
        <Button on:click={() => onSubmit({title, image: imagePreview, ingredientsText, methodStepsText, tags})}>
          {recipe ? 'Update Recipe' : 'Add Recipe'}
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>