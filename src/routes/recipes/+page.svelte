<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as Card from "$lib/components/ui/card";
  import MainLayout from "$lib/components/MainLayout.svelte";

  // Sample data for recipes (replace with actual data later)
  const recipes = [
    {
      id: 1,
      title: "Spaghetti Carbonara",
      image: "https://example.com/carbonara.jpg",
      tags: ["Italian", "Pasta", "Quick"]
    },
    {
      id: 2,
      title: "Chicken Stir Fry",
      image: "https://example.com/stir-fry.jpg",
      tags: ["Asian", "Chicken", "Healthy"]
    },
    // Add more recipes as needed
  ];

  let isInputFocused = false;
  let inputValue = "";

  function handleFocus() {
    isInputFocused = true;
  }

  function handleBlur() {
    isInputFocused = false;
  }

  async function handleSubmit(event: Event) {
    event.preventDefault();
    if (inputValue.trim()) {
      try {
        const response = await fetch('/api/recipes', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ recipe: inputValue }),
        });

        if (response.ok) {
          console.log('Recipe submitted successfully');
          inputValue = ""; // Clear the input after successful submission
        } else {
          console.error('Failed to submit recipe');
        }
      } catch (error) {
        console.error('Error submitting recipe:', error);
      }
    }
  }
</script>

<MainLayout>
  <div class="container mx-auto py-8">
    <form on:submit={handleSubmit} class="mb-8 flex justify-center relative input-wrapper" class:focused={isInputFocused}>
      <Input
        type="text"
        placeholder="Add a new recipe..."
        class="w-full max-w-xl rounded-full transition-all duration-300"
        on:focus={handleFocus}
        on:blur={handleBlur}
        bind:value={inputValue}
      />
    </form>

    <div class="content" class:blurred={isInputFocused}>
      <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
    {#each recipes as recipe}
      <Card.Root>
        <Card.Header class="p-0">
          <img src={recipe.image} alt={recipe.title} class="h-48 w-full object-cover" />
        </Card.Header>
        <Card.Content class="p-4 text-right">
          <Card.Title class="text-2xl mb-3">{recipe.title}</Card.Title>
          <div class="mt-2 flex flex-wrap justify-end gap-2">
            {#each recipe.tags as tag}
              <span class="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold text-primary-foreground">
                {tag}
              </span>
            {/each}
          </div>
        </Card.Content>
      </Card.Root>
    {/each}
  </div>
  </div>
</MainLayout>

<style>
  .input-wrapper {
    z-index: 10;
    transition: all 0.3s ease-in-out;
  }

  .input-wrapper.focused :global(input) {
    transform: scale(1.2);
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  }

  .content {
    transition: filter 0.3s ease-in-out;
  }

  .content.blurred {
    filter: blur(5px);
  }
</style>
