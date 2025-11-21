# Component Patterns

Modern Svelte 5 component architecture emphasizing runes-based reactivity, type safety, and proper component composition.

---

## Svelte 5 Runes Pattern

### Core Runes

Svelte 5 introduces runes for explicit reactivity:
- `$state()` - Reactive state
- `$derived()` - Computed values
- `$effect()` - Side effects
- `$props()` - Component props
- `$bindable()` - Two-way bindable props

### Basic Component Pattern

```svelte
<script lang="ts">
  interface Props {
    /** User ID to display */
    userId: number;
    /** Optional callback when action occurs */
    onAction?: () => void;
  }
  
  let { userId, onAction }: Props = $props();
  
  // Reactive state
  let count = $state(0);
  let userData = $state<User | null>(null);
  
  // Computed values
  const doubled = $derived(count * 2);
  const userName = $derived(userData?.name ?? 'Anonymous');
  
  // Side effects
  $effect(() => {
    console.log(`Count changed to ${count}`);
  });
  
  function handleClick() {
    count++;
    onAction?.();
  }
</script>

<div>
  User: {userId} ({userName})
  <button onclick={handleClick}>
    Count: {count} (doubled: {doubled})
  </button>
</div>
```

**Key Points:**
- Use `<script lang="ts">` for TypeScript
- Define Props interface with JSDoc comments
- Destructure props with `$props()`
- Use runes for reactivity

---

## State Management with Runes

### Local State

```svelte
<script lang="ts">
  // Simple state
  let count = $state(0);
  let name = $state('');
  
  // Complex state
  let user = $state<User | null>(null);
  let items = $state<Item[]>([]);
  
  // Deep reactivity (automatic)
  let todos = $state([
    { id: 1, text: 'Learn Svelte', done: false }
  ]);
  
  // This will trigger updates
  todos[0].done = true;
  todos.push({ id: 2, text: 'Build app', done: false });
</script>
```

### Non-reactive State

```svelte
<script lang="ts">
  // Use $state.raw for large, immutable data
  let bigData = $state.raw({
    // Large dataset that won't be mutated
    items: Array(10000).fill(0).map((_, i) => ({ id: i, value: i }))
  });
  
  // Must reassign entire object to trigger updates
  bigData = { ...bigData, items: newItems };
</script>
```

### Shared State (Store Pattern)

```typescript
// stores/user.svelte.ts
export const userState = $state({
  name: '',
  email: '',
  isLoggedIn: false
});

// Component.svelte
<script lang="ts">
  import { userState } from '$lib/stores/user.svelte.ts';
</script>

<p>Welcome, {userState.name}!</p>
<button onclick={() => userState.isLoggedIn = false}>
  Logout
</button>
```

---

## Derived Values

### Basic Derived

```svelte
<script lang="ts">
  let width = $state(10);
  let height = $state(20);
  
  // Simple derived
  const area = $derived(width * height);
  
  // With logic
  const status = $derived({
    if (area > 100) return 'large';
    if (area > 50) return 'medium';
    return 'small';
  });
</script>
```

### Complex Derived

```svelte
<script lang="ts">
  let searchQuery = $state('');
  let items = $state<Item[]>([]);
  
  // Filtered and sorted
  const filteredItems = $derived(
    items
      .filter(item => 
        item.name.toLowerCase().includes(searchQuery.toLowerCase())
      )
      .sort((a, b) => a.name.localeCompare(b.name))
  );
  
  // With async (use $derived.by)
  const processedData = $derived.by(() => {
    // Complex synchronous computation
    return processItems(filteredItems);
  });
</script>
```

---

## Effects and Lifecycle

### Basic Effects

```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  
  let count = $state(0);
  
  // Runs when dependencies change
  $effect(() => {
    console.log(`Count is now ${count}`);
    
    // Cleanup function (optional)
    return () => {
      console.log('Cleaning up effect');
    };
  });
  
  // Pre-effect (runs before DOM update)
  $effect.pre(() => {
    console.log('Before DOM update');
  });
  
  // Component lifecycle
  onMount(() => {
    console.log('Component mounted');
    
    // Return cleanup
    return () => {
      console.log('Component unmounting');
    };
  });
  
  onDestroy(() => {
    console.log('Component destroyed');
  });
</script>
```

### Async Effects

```svelte
<script lang="ts">
  let userId = $state(1);
  let userData = $state<User | null>(null);
  
  // Async data fetching in effect
  $effect(() => {
    let cancelled = false;
    
    async function fetchUser() {
      const response = await fetch(`/api/users/${userId}`);
      const data = await response.json();
      
      if (!cancelled) {
        userData = data;
      }
    }
    
    fetchUser();
    
    return () => {
      cancelled = true;
    };
  });
</script>
```

---

## Props and Binding

### Basic Props

```svelte
<script lang="ts">
  interface Props {
    name: string;
    age?: number;
    onUpdate?: (value: string) => void;
  }
  
  let { 
    name, 
    age = 0,
    onUpdate 
  }: Props = $props();
</script>

<div>
  {name} is {age} years old
</div>
```

### Bindable Props

```svelte
<!-- Child.svelte -->
<script lang="ts">
  interface Props {
    value: string;
  }
  
  let { value = $bindable() }: Props = $props();
</script>

<input bind:value />

<!-- Parent.svelte -->
<script lang="ts">
  import Child from './Child.svelte';
  let text = $state('');
</script>

<Child bind:value={text} />
<p>Text: {text}</p>
```

### Rest Props

```svelte
<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  
  let { children, ...rest }: HTMLButtonAttributes = $props();
</script>

<button {...rest}>
  {@render children?.()}
</button>
```

---

## Slots and Snippets

### Basic Slots (Legacy)

```svelte
<!-- Card.svelte -->
<div class="card">
  <slot name="header" />
  <slot />
  <slot name="footer" />
</div>

<!-- Usage -->
<Card>
  <h2 slot="header">Title</h2>
  Main content
  <div slot="footer">Footer</div>
</Card>
```

### Snippets (Modern)

```svelte
<!-- List.svelte -->
<script lang="ts">
  interface Props {
    items: Item[];
    item: (item: Item) => any;
    empty?: () => any;
  }
  
  let { items, item, empty }: Props = $props();
</script>

{#if items.length}
  <ul>
    {#each items as entry}
      <li>{@render item(entry)}</li>
    {/each}
  </ul>
{:else if empty}
  {@render empty()}
{/if}

<!-- Usage -->
<List {items}>
  {#snippet item(data)}
    <span>{data.name}</span>
  {/snippet}
  {#snippet empty()}
    <p>No items found</p>
  {/snippet}
</List>
```

---

## Component Structure Template

### Recommended Order

```svelte
<script lang="ts">
  // 1. IMPORTS
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import Button from '$lib/components/Button.svelte';
  import type { User } from '$lib/types';
  
  // 2. PROPS INTERFACE
  interface Props {
    /** User ID to load */
    userId: number;
    /** Optional callback */
    onComplete?: () => void;
  }
  
  // 3. PROPS DESTRUCTURING
  let { userId, onComplete }: Props = $props();
  
  // 4. STATE DECLARATIONS
  let user = $state<User | null>(null);
  let loading = $state(true);
  let error = $state<Error | null>(null);
  
  // 5. DERIVED VALUES
  const fullName = $derived(
    user ? `${user.firstName} ${user.lastName}` : 'Unknown'
  );
  
  // 6. EFFECTS
  $effect(() => {
    loadUser(userId);
  });
  
  // 7. LIFECYCLE HOOKS
  onMount(() => {
    console.log('Component mounted');
  });
  
  // 8. FUNCTIONS
  async function loadUser(id: number) {
    try {
      loading = true;
      const response = await fetch(`/api/users/${id}`);
      if (!response.ok) throw new Error('Failed to load user');
      user = await response.json();
    } catch (err) {
      error = err as Error;
    } finally {
      loading = false;
    }
  }
  
  function handleSave() {
    // Save logic
    onComplete?.();
  }
</script>

<!-- 9. MARKUP -->
{#if loading}
  <div class="loading">Loading...</div>
{:else if error}
  <div class="error">{error.message}</div>
{:else if user}
  <div class="user-card">
    <h2>{fullName}</h2>
    <Button onclick={handleSave}>Save</Button>
  </div>
{/if}

<!-- 10. STYLES -->
<style>
  .user-card {
    padding: 1rem;
    border-radius: 8px;
    background: var(--surface);
  }
  
  .loading {
    color: var(--text-secondary);
  }
  
  .error {
    color: var(--error);
  }
</style>
```

---

## Generic Components

### Type-Safe Generic Components

```svelte
<script lang="ts" generics="T extends { id: string, name: string }">
  interface Props {
    items: T[];
    selected?: T;
    onSelect: (item: T) => void;
  }
  
  let { items, selected, onSelect }: Props = $props();
</script>

<div class="list">
  {#each items as item (item.id)}
    <button
      class:selected={selected?.id === item.id}
      onclick={() => onSelect(item)}
    >
      {item.name}
    </button>
  {/each}
</div>

<style>
  .selected {
    background: var(--primary);
    color: white;
  }
</style>
```

---

## Component Communication

### Props Down, Events Up

```svelte
<!-- Parent.svelte -->
<script lang="ts">
  import Child from './Child.svelte';
  
  let selectedId = $state<string | null>(null);
  let data = $state([...]);
  
  function handleSelect(id: string) {
    selectedId = id;
  }
</script>

<Child {data} onSelect={handleSelect} />

<!-- Child.svelte -->
<script lang="ts">
  interface Props {
    data: Data[];
    onSelect: (id: string) => void;
  }
  
  let { data, onSelect }: Props = $props();
</script>

<button onclick={() => onSelect(data[0].id)}>
  Select First
</button>
```

### Context API

```svelte
<!-- Provider.svelte -->
<script lang="ts">
  import { setContext } from 'svelte';
  
  const userContext = $state({
    name: 'John',
    role: 'admin'
  });
  
  setContext('user', userContext);
</script>

<slot />

<!-- Consumer.svelte -->
<script lang="ts">
  import { getContext } from 'svelte';
  
  const user = getContext<UserContext>('user');
</script>

<p>Hello, {user.name}!</p>
```

---

## Performance Patterns

### Conditional Rendering

```svelte
<script lang="ts">
  let showDetails = $state(false);
  let heavyData = $state<Data | null>(null);
  
  // Load data only when needed
  $effect(() => {
    if (showDetails && !heavyData) {
      loadHeavyData().then(data => heavyData = data);
    }
  });
</script>

{#if showDetails}
  {#if heavyData}
    <HeavyComponent data={heavyData} />
  {:else}
    <p>Loading details...</p>
  {/if}
{/if}
```

### Debounced Updates

```svelte
<script lang="ts">
  let searchInput = $state('');
  let searchResults = $state<Result[]>([]);
  let debounceTimer: number;
  
  $effect(() => {
    clearTimeout(debounceTimer);
    
    if (searchInput) {
      debounceTimer = setTimeout(() => {
        performSearch(searchInput).then(results => {
          searchResults = results;
        });
      }, 300);
    } else {
      searchResults = [];
    }
    
    return () => clearTimeout(debounceTimer);
  });
</script>

<input bind:value={searchInput} placeholder="Search..." />
```

---

## Summary

**Modern Svelte 5 Component Recipe:**
1. Use runes (`$state`, `$derived`, `$effect`) for reactivity
2. Type props with interfaces and `$props()`
3. Structure: Script → Markup → Style
4. Use snippets for content projection
5. Context API for deep prop passing
6. Leverage Svelte's compile-time optimizations
7. Scoped styles by default
8. No virtual DOM = better performance

**See Also:**
- [data-fetching.md](data-fetching.md) - SvelteKit data loading
- [loading-and-error-states.md](loading-and-error-states.md) - Error boundaries
- [complete-examples.md](complete-examples.md) - Full working examples