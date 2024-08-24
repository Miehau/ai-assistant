<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as Card from "$lib/components/ui/card";

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
  import MainLayout from "$lib/components/MainLayout.svelte";
</script>

<MainLayout>
  <div class="container mx-auto py-8">
  <div class="mb-8 flex justify-center">
      <Input type="text" placeholder="Add a new recipe..." class="w-full max-w-xl rounded-full" />
  </div>

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
