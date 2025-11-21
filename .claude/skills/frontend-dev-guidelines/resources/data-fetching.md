# Data Fetching

Comprehensive guide for data fetching in SvelteKit applications using load functions, form actions, and client-side patterns.

---

## SvelteKit Load Functions

### Page Data Loading

**Universal Load (+page.ts):**
```typescript
// src/routes/blog/[slug]/+page.ts
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, fetch, depends }) => {
    const response = await fetch(`/api/posts/${params.slug}`);
    
    if (!response.ok) {
        throw error(404, 'Post not found');
    }
    
    const post = await response.json();
    
    // Mark dependency for invalidation
    depends('post:detail');
    
    return {
        post,
        // Data available in +page.svelte as `data.post`
    };
};
```

**Server-Only Load (+page.server.ts):**
```typescript
// src/routes/users/+page.server.ts
import type { PageServerLoad } from './$types';
import { db } from '$lib/server/db';

export const load: PageServerLoad = async ({ cookies, locals }) => {
    // Access server-only resources
    const sessionId = cookies.get('session');
    const user = locals.user;
    
    // Direct database access
    const users = await db.user.findMany({
        where: { active: true }
    });
    
    return {
        users,
        currentUser: user
    };
};
```

### Layout Data Loading

```typescript
// src/routes/+layout.ts
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ fetch, url }) => {
    // Load data available to all child routes
    const navigation = await fetch('/api/navigation').then(r => r.json());
    
    return {
        navigation,
        currentPath: url.pathname
    };
};
```

---

## Using Loaded Data

### In Components

```svelte
<!-- +page.svelte -->
<script lang="ts">
  import type { PageData } from './$types';
  
  // Automatically typed from load function
  let { data }: { data: PageData } = $props();
</script>

<article>
  <h1>{data.post.title}</h1>
  <div>{@html data.post.content}</div>
</article>
```

### Accessing Parent Data

```svelte
<script lang="ts">
  import type { PageData } from './$types';
  
  let { data }: { data: PageData } = $props();
  
  // Access both page and layout data
  // data.post (from +page.ts)
  // data.navigation (from +layout.ts)
</script>
```

---

## Form Actions

### Server-Side Form Handling

```typescript
// +page.server.ts
import type { Actions } from './$types';
import { fail, redirect } from '@sveltejs/kit';

export const actions = {
    create: async ({ request, cookies }) => {
        const formData = await request.formData();
        const title = formData.get('title') as string;
        const content = formData.get('content') as string;
        
        // Validation
        if (!title || !content) {
            return fail(400, {
                title,
                content,
                error: 'All fields are required'
            });
        }
        
        // Create in database
        try {
            const post = await db.post.create({
                data: { title, content }
            });
            
            // Redirect after success
            throw redirect(303, `/posts/${post.id}`);
        } catch (error) {
            return fail(500, {
                title,
                content,
                error: 'Failed to create post'
            });
        }
    },
    
    delete: async ({ request }) => {
        const formData = await request.formData();
        const id = formData.get('id') as string;
        
        await db.post.delete({ where: { id } });
        
        return { success: true };
    }
} satisfies Actions;
```

### Using Form Actions in Components

```svelte
<script lang="ts">
  import { enhance } from '$app/forms';
  import type { ActionData } from './$types';
  
  let { form }: { form: ActionData } = $props();
</script>

<!-- Basic form -->
<form method="POST" action="?/create">
  <input name="title" value={form?.title ?? ''} />
  <textarea name="content" value={form?.content ?? ''} />
  
  {#if form?.error}
    <p class="error">{form.error}</p>
  {/if}
  
  <button>Create Post</button>
</form>

<!-- Enhanced form with progressive enhancement -->
<form method="POST" action="?/delete" use:enhance={() => {
  return async ({ result, update }) => {
    if (result.type === 'success') {
      // Custom handling
      console.log('Deleted successfully');
    }
    // Apply default behavior
    update();
  };
}}>
  <input type="hidden" name="id" value={post.id} />
  <button>Delete</button>
</form>
```

---

## Client-Side Fetching

### Using Native Fetch

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  
  let posts = $state<Post[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  
  async function loadPosts() {
    loading = true;
    error = null;
    
    try {
      const response = await fetch('/api/posts');
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      posts = await response.json();
    } catch (err) {
      error = err.message;
    } finally {
      loading = false;
    }
  }
  
  onMount(() => {
    loadPosts();
  });
</script>

{#if loading}
  <p>Loading posts...</p>
{:else if error}
  <p>Error: {error}</p>
{:else}
  <ul>
    {#each posts as post}
      <li>{post.title}</li>
    {/each}
  </ul>
{/if}
```

### Reactive Fetching

```svelte
<script lang="ts">
  let searchQuery = $state('');
  let results = $state<SearchResult[]>([]);
  let controller: AbortController;
  
  // Fetch when query changes
  $effect(() => {
    // Cancel previous request
    controller?.abort();
    
    if (searchQuery.length < 3) {
      results = [];
      return;
    }
    
    controller = new AbortController();
    
    fetch(`/api/search?q=${searchQuery}`, {
      signal: controller.signal
    })
      .then(r => r.json())
      .then(data => results = data)
      .catch(err => {
        if (err.name !== 'AbortError') {
          console.error('Search failed:', err);
        }
      });
  });
</script>

<input bind:value={searchQuery} placeholder="Search..." />
```

---

## Data Invalidation

### Manual Invalidation

```typescript
import { invalidate, invalidateAll } from '$app/navigation';

// Invalidate specific dependencies
await invalidate('post:detail');
await invalidate((url) => url.pathname.startsWith('/api/posts'));

// Invalidate all data
await invalidateAll();
```

### Automatic Invalidation

```svelte
<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  
  // After form submission
  async function handleSubmit() {
    const response = await fetch('/api/posts', {
      method: 'POST',
      body: JSON.stringify(data)
    });
    
    if (response.ok) {
      // Refresh all load functions
      await invalidateAll();
    }
  }
</script>
```

---

## API Routes

### Creating API Endpoints

```typescript
// src/routes/api/posts/+server.ts
import type { RequestHandler } from './$types';
import { json, error } from '@sveltejs/kit';

export const GET: RequestHandler = async ({ url, locals }) => {
    const limit = Number(url.searchParams.get('limit') ?? 10);
    
    // Check authentication
    if (!locals.user) {
        throw error(401, 'Unauthorized');
    }
    
    const posts = await db.post.findMany({
        take: limit,
        orderBy: { createdAt: 'desc' }
    });
    
    return json(posts);
};

export const POST: RequestHandler = async ({ request, locals }) => {
    const data = await request.json();
    
    // Validate
    if (!data.title || !data.content) {
        throw error(400, 'Missing required fields');
    }
    
    const post = await db.post.create({
        data: {
            ...data,
            authorId: locals.user.id
        }
    });
    
    return json(post, { status: 201 });
};
```

### Consuming API Routes

```svelte
<script lang="ts">
  // From load function (recommended)
  let { data } = $props();
  
  // Client-side fetch
  async function createPost(title: string, content: string) {
    const response = await fetch('/api/posts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title, content })
    });
    
    if (!response.ok) {
      throw new Error('Failed to create post');
    }
    
    return response.json();
  }
</script>
```

---

## Streaming SSR

### Progressive Loading

```typescript
// +page.server.ts
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
    return {
        // Immediate data
        title: 'Posts',
        
        // Streamed promise
        posts: loadPosts(), // Returns Promise<Post[]>
        
        // Another streamed promise
        comments: loadComments()
    };
};
```

```svelte
<!-- +page.svelte -->
<script lang="ts">
  let { data } = $props();
</script>

<h1>{data.title}</h1>

{#await data.posts}
  <p>Loading posts...</p>
{:then posts}
  {#each posts as post}
    <article>{post.title}</article>
  {/each}
{:catch error}
  <p>Error loading posts: {error.message}</p>
{/await}
```

---

## Store-Based Data Management

### Writable Store Pattern

```typescript
// stores/posts.ts
import { writable, derived } from 'svelte/store';

function createPostStore() {
    const { subscribe, set, update } = writable<Post[]>([]);
    
    return {
        subscribe,
        
        async load() {
            const response = await fetch('/api/posts');
            const posts = await response.json();
            set(posts);
        },
        
        async create(post: NewPost) {
            const response = await fetch('/api/posts', {
                method: 'POST',
                body: JSON.stringify(post)
            });
            const created = await response.json();
            update(posts => [...posts, created]);
            return created;
        },
        
        async delete(id: string) {
            await fetch(`/api/posts/${id}`, { method: 'DELETE' });
            update(posts => posts.filter(p => p.id !== id));
        }
    };
}

export const posts = createPostStore();
```

### Using Data Stores

```svelte
<script lang="ts">
  import { posts } from '$lib/stores/posts';
  import { onMount } from 'svelte';
  
  onMount(() => {
    posts.load();
  });
  
  async function handleCreate() {
    await posts.create({ title: 'New Post', content: '...' });
  }
</script>

{#each $posts as post}
  <article>
    <h2>{post.title}</h2>
    <button onclick={() => posts.delete(post.id)}>Delete</button>
  </article>
{/each}
```

---

## Error Handling

### In Load Functions

```typescript
// +page.ts
import { error } from '@sveltejs/kit';

export const load: PageLoad = async ({ fetch, params }) => {
    const response = await fetch(`/api/posts/${params.id}`);
    
    if (!response.ok) {
        if (response.status === 404) {
            throw error(404, 'Post not found');
        }
        throw error(response.status, 'Failed to load post');
    }
    
    return {
        post: await response.json()
    };
};
```

### Error Pages

```svelte
<!-- +error.svelte -->
<script lang="ts">
  import { page } from '$app/stores';
</script>

<h1>{$page.status}</h1>
<p>{$page.error?.message}</p>

{#if $page.status === 404}
  <a href="/">Go to homepage</a>
{:else}
  <button onclick={() => location.reload()}>Try again</button>
{/if}
```

---

## Best Practices

### 1. Prefer Server-Side Loading
```typescript
// ✅ GOOD - Server-side loading
export const load: PageServerLoad = async () => {
    const posts = await db.post.findMany();
    return { posts };
};

// ❌ AVOID - Client-side loading for initial data
onMount(async () => {
    const posts = await fetch('/api/posts');
    // ...
});
```

### 2. Use Proper Error Handling
```typescript
// ✅ GOOD - Proper error handling
try {
    const data = await fetchData();
    return { data };
} catch (err) {
    throw error(500, 'Failed to load data');
}
```

### 3. Cancel Ongoing Requests
```typescript
// ✅ GOOD - Request cancellation
let controller: AbortController;

$effect(() => {
    controller?.abort();
    controller = new AbortController();
    
    fetch(url, { signal: controller.signal })
        .then(handleResponse)
        .catch(handleError);
});
```

### 4. Type Your Data
```typescript
// ✅ GOOD - Typed responses
interface Post {
    id: string;
    title: string;
    content: string;
}

const response = await fetch('/api/posts');
const posts: Post[] = await response.json();
```

---

## Summary

**SvelteKit Data Fetching Patterns:**
1. Use load functions for initial data
2. Form actions for mutations
3. Client-side fetch for dynamic updates
4. Invalidation for cache refresh
5. Streaming SSR for progressive enhancement
6. Stores for client-side state
7. Proper error handling with error pages
8. Type everything with TypeScript

**See Also:**
- [routing-guide.md](routing-guide.md) - SvelteKit routing
- [loading-and-error-states.md](loading-and-error-states.md) - Loading patterns
- [complete-examples.md](complete-examples.md) - Full examples