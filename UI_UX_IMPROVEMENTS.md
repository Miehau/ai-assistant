# AI Frontend - UI/UX Modernization Plan
## Raycast/Arc Style Transformation

**Target Aesthetic:** Glassmorphism, depth, vibrant colors, modern Mac-native feel
**Current State:** Flat dark theme with basic green accents
**Goal:** Transform into a premium, polished AI chat experience

---

## Phase 1: Visual Design Foundation 🎨

### 1.1 Glassmorphism Design System

**What to implement:**
- Frosted glass effect using `backdrop-blur` and semi-transparent backgrounds
- Layered elevation system with proper z-index hierarchy
- Subtle noise/grain texture overlay for depth
- Translucent panels that show underlying content

**Technical approach:**
```css
/* Example glass effect utilities */
.glass-panel {
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: 0 8px 32px 0 rgba(0, 0, 0, 0.37);
}

.glass-dark {
  background: rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(40px);
}
```

**Files to modify:**
- `src/app.css` - Add new utility classes
- `tailwind.config.js` - Extend with glass effect utilities
- `src/lib/components/MainLayout.svelte` - Apply glass to sidebar
- `src/lib/components/Navbar.svelte` - Frosted glass navigation

**Visual reference:**
- Sidebar: Semi-transparent with blur, floating on background
- Cards/Panels: Layered glass with subtle borders
- Modals/Drawers: Heavy blur with darker tint
- Dropdowns: Light glass with shadow

---

### 1.2 Vibrant Gradient System

**What to implement:**
- Multi-color gradient palette (green → cyan → purple → amber)
- Animated gradient backgrounds for hero sections
- Gradient borders on interactive elements
- Glow effects using CSS filters and shadows

**Color palette additions:**
```javascript
// New gradient definitions
colors: {
  primary: {
    DEFAULT: '#52b788', // Current green
    gradient: 'linear-gradient(135deg, #52b788 0%, #06ffa5 100%)',
  },
  accent: {
    cyan: '#22d3ee',
    purple: '#a855f7',
    amber: '#fbbf24',
  },
  glow: {
    green: 'rgba(82, 183, 136, 0.5)',
    cyan: 'rgba(34, 211, 238, 0.5)',
    purple: 'rgba(168, 85, 247, 0.5)',
  }
}
```

**Where to apply:**
- Primary buttons: Green-to-cyan gradient
- Active states: Glowing border with colored shadow
- Model badges: Color-coded gradients (Vision = cyan, Reasoning = amber, etc.)
- Success states: Green glow
- Code blocks: Subtle gradient header bar

**Files to modify:**
- `tailwind.config.js` - Add gradient utilities
- `src/app.css` - Add glow effect classes
- `src/lib/components/ui/button/button.svelte` - Gradient variants
- `src/lib/components/chat/ChatControls.svelte` - Apply to model badges

---

### 1.3 Elevation & Shadow System

**What to implement:**
- 4-tier shadow system: `shadow-low`, `shadow-medium`, `shadow-high`, `shadow-float`
- Colored shadows matching element colors
- Hover state elevation changes
- Smooth shadow transitions

**Shadow scale:**
```javascript
boxShadow: {
  'low': '0 2px 8px rgba(0, 0, 0, 0.1)',
  'medium': '0 4px 16px rgba(0, 0, 0, 0.15)',
  'high': '0 8px 32px rgba(0, 0, 0, 0.2)',
  'float': '0 16px 48px rgba(0, 0, 0, 0.3)',
  'glow-green': '0 0 20px rgba(82, 183, 136, 0.5)',
  'glow-cyan': '0 0 20px rgba(34, 211, 238, 0.5)',
}
```

**Where to apply:**
- Cards: shadow-medium at rest, shadow-high on hover
- Modals: shadow-float for prominent elevation
- Buttons: shadow-low, with colored glow on hover
- Floating action buttons: shadow-high with bounce animation

---

### 1.4 Component Visual Refresh

#### Sidebar (Navbar.svelte)
**Current:** Flat dark background, icon-only
**New design:**
- Frosted glass background with blur
- Floating pill-style active indicator with gradient
- Icons with subtle color coding
- Hover glow effect
- Expand/collapse functionality with smooth animation
- Show labels on hover or when expanded

**Mockup concept:**
```
┌─────────────────────┐
│  [≡]  <collapsed>   │ ← Frosted glass panel
│                     │
│  [🏠] ← Active      │ ← Glowing gradient pill
│  [</>]             │ ← Hover shows label
│  [👥]              │
│  [📜]              │
│  [?]               │
└─────────────────────┘
```

#### Message Bubbles
**Current:** Solid background, rounded corners
**New design:**
- Glass effect background
- Colored gradient left border (green for user, cyan for AI)
- Subtle glow shadow
- Hover reveals action buttons (copy, edit, regenerate, branch)
- Timestamp badge (bottom-right, semi-transparent)
- Model avatar icon (top-left for AI messages)

**Layout:**
```
┌─ [AI Avatar] ─────────────────────────────┐
│ │ Message content here...                │ │ ← Glass background
│ │                                         │ │ ← Cyan glow border
│ │ Code blocks, images, etc.              │ │
│ └─────────────────── [2m ago] [⋮ Actions] │ ← Hover actions
└───────────────────────────────────────────┘
```

#### Buttons
**Current:** Solid green or ghost variants
**New design:**
- Primary: Vibrant gradient background (green → cyan)
- Hover: Elevated with glow effect
- Ghost: Transparent with border, gradient on hover
- Icon buttons: Glass background with hover glow
- Loading state: Animated gradient shimmer

#### Input Fields
**Current:** Basic border, solid background
**New design:**
- Frosted glass background
- Subtle inner glow
- Focus state: Colored border with outer glow
- Animated border gradient on focus
- Character count indicator with color coding

#### Code Blocks
**Current:** Dark background with syntax highlighting
**New design:**
- Glass container with darker tint
- Gradient top bar (language label + copy button)
- Hover: Border glow effect
- Copy button: Glass with success animation (checkmark + green glow)
- Line numbers with gradient background

---

## Phase 2: AI-Native Features 🤖

### 2.1 Token & Cost Tracking

**What to implement:**

#### Real-time Token Counter
- **Location:** Bottom-right of chat input textarea
- **Display:** Current message tokens / Model context limit
- **Visual:** Small glass badge with gradient border
- **Color coding:**
  - Green (0-50% of limit)
  - Amber (50-80% of limit)
  - Red (80-100% of limit)

**Implementation:**
```svelte
<!-- In ChatInput.svelte -->
<div class="token-counter glass-badge">
  <span class="text-xs">
    {currentTokens} / {modelLimit} tokens
  </span>
  <div class="token-bar" style="width: {percentage}%"></div>
</div>
```

#### Cost Estimator
- **Display:** Estimated cost per message (hover tooltip)
- **Running total:** Conversation total cost (in ChatControls)
- **Visual:** Glass badge with $ icon, color-coded by cost
- **Calculation:** Based on model pricing (input + output tokens)

**Data structure:**
```typescript
interface TokenUsage {
  conversationId: string;
  messages: {
    id: string;
    inputTokens: number;
    outputTokens: number;
    cost: number;
    model: string;
    timestamp: Date;
  }[];
  totalCost: number;
  totalTokens: number;
}
```

#### Usage Dashboard
- **Location:** New route `/usage` or modal overlay
- **Charts:**
  - Cost over time (line chart)
  - Tokens by model (pie chart)
  - Conversations by cost (bar chart)
  - Daily/weekly/monthly breakdown
- **Export:** CSV/JSON download
- **Filters:** Date range, model, conversation

**Libraries to add:**
```bash
npm install chart.js svelte-chartjs
# or
npm install recharts
```

**Files to create/modify:**
- `src/lib/stores/tokenUsage.ts` - Svelte store for tracking
- `src/lib/components/chat/TokenCounter.svelte` - Counter component
- `src/lib/components/chat/CostEstimator.svelte` - Cost display
- `src/routes/usage/+page.svelte` - Usage dashboard
- `src/lib/utils/tokenCalculator.ts` - Token counting utility
- `src/lib/utils/costCalculator.ts` - Cost calculation utility

**Token counting approach:**
- Use `tiktoken` library for accurate token counting
- Cache model context limits
- Update count on every input change (debounced)

---

### 2.2 Model Comparison Mode

**What to implement:**

#### Split-Screen Layout
- **Trigger:** Toggle button in ChatControls ("Compare mode")
- **Layout:** 2-4 column grid, side-by-side responses
- **Each column:**
  - Model selector dropdown
  - Independent message stream
  - Performance metrics (response time, tokens)
  - Cost comparison

**Visual layout:**
```
┌─────────────────┬─────────────────┬─────────────────┐
│ GPT-4           │ Claude Sonnet   │ DeepSeek        │
│ [≡] Settings    │ [≡] Settings    │ [≡] Settings    │
├─────────────────┼─────────────────┼─────────────────┤
│                 │                 │                 │
│ Response 1      │ Response 2      │ Response 3      │
│                 │                 │                 │
│ 2.3s | $0.05   │ 1.8s | $0.03   │ 3.1s | $0.01   │
└─────────────────┴─────────────────┴─────────────────┘
       ┌──────────────────────────┐
       │ Shared input field       │
       └──────────────────────────┘
```

#### Features:
1. **Synchronized scrolling** (toggle on/off)
2. **Diff highlighting** - Show differences between responses
3. **Performance metrics table:**
   - Response time
   - Token count
   - Cost
   - Quality rating (user can rate)
4. **Export comparison** - Save as markdown table
5. **Winner selection** - Highlight preferred response

**Implementation details:**
```svelte
<!-- ComparisonView.svelte -->
<script>
  let comparisonModels = ['gpt-4', 'claude-sonnet-3.5', 'deepseek-chat'];
  let responses = [];
  let syncScroll = true;

  async function sendToAllModels(prompt) {
    responses = await Promise.all(
      comparisonModels.map(model =>
        sendMessage(prompt, model)
      )
    );
  }
</script>

<div class="comparison-grid grid grid-cols-{comparisonModels.length}">
  {#each comparisonModels as model, i}
    <div class="comparison-column glass-panel">
      <ModelSelector bind:value={comparisonModels[i]} />
      <MessageDisplay messages={responses[i]} />
      <MetricsDisplay data={responses[i].metrics} />
    </div>
  {/each}
</div>
```

**Files to create/modify:**
- `src/routes/compare/+page.svelte` - Comparison view
- `src/lib/components/comparison/ComparisonGrid.svelte`
- `src/lib/components/comparison/ModelColumn.svelte`
- `src/lib/components/comparison/MetricsDisplay.svelte`
- `src/lib/components/comparison/DiffHighlighter.svelte`
- `src/lib/utils/diffChecker.ts` - Text diff algorithm

---

### 2.3 Conversation Branching

**What to implement:**

#### Branch Creation
- **Trigger:** Icon button on any message (tree/fork icon)
- **Action:** Create new conversation branch from that point
- **Visual:** Branch indicator badge on message
- **Behavior:** Copies conversation up to that message into new branch

**Branch Management:**
```typescript
interface ConversationBranch {
  id: string;
  parentBranchId: string | null;
  branchPoint: number; // Message index where branch occurred
  name: string;
  createdAt: Date;
  messages: Message[];
}
```

#### Branch Switcher
- **Location:** Dropdown in ChatControls
- **Display:** List of all branches with metadata
  - Branch name
  - Message count
  - Created date
  - Branch point indicator
- **Visual:** Glass dropdown with tree structure visualization

**Branch tree view:**
```
Main Conversation
├─ Branch 1: "Try different approach" (from msg #3)
├─ Branch 2: "Alternative solution" (from msg #5)
│  └─ Branch 2.1: "Refined version" (from msg #8)
└─ Branch 3: "Debugging path" (from msg #4)
```

#### Branch Comparison
- **Feature:** Compare two branches side-by-side
- **Layout:** Similar to model comparison
- **Diff:** Highlight where branches diverged
- **Merge:** Option to copy messages from one branch to another

**UI Components:**
```svelte
<!-- BranchIndicator.svelte -->
<button class="branch-button glass-badge">
  <TreeIcon size={16} />
  <span>{branchCount} branches</span>
</button>

<!-- BranchSwitcher.svelte -->
<Select>
  <SelectTrigger>Current: {currentBranch.name}</SelectTrigger>
  <SelectContent>
    {#each branches as branch}
      <SelectItem value={branch.id}>
        <div class="branch-item">
          <TreeIcon />
          <span>{branch.name}</span>
          <span class="text-xs text-muted">
            {branch.messages.length} messages
          </span>
        </div>
      </SelectItem>
    {/each}
  </SelectContent>
</Select>
```

**Files to create/modify:**
- `src/lib/stores/branches.ts` - Branch management store
- `src/lib/components/chat/BranchIndicator.svelte`
- `src/lib/components/chat/BranchSwitcher.svelte`
- `src/lib/components/chat/BranchTreeView.svelte` - Visual tree
- `src/lib/components/chat/BranchComparison.svelte`
- `src/lib/utils/branchManager.ts` - Branch operations

---

## Phase 3: Enhanced Interactions ⚡

### 3.1 Rich Message UI

**What to implement:**

#### Message Timestamps
- **Display:** Relative time ("2m ago", "1h ago", "yesterday")
- **Hover:** Shows absolute timestamp in tooltip
- **Position:** Bottom-right of message bubble
- **Visual:** Small glass badge, semi-transparent

```svelte
<div class="message-timestamp glass-badge-sm">
  <span title={absoluteTime}>{relativeTime}</span>
</div>
```

#### Model Avatars
- **For AI messages:** Show model icon/logo
- **Color coding:** Each model family gets a color
  - OpenAI: Green
  - Anthropic: Orange
  - DeepSeek: Blue
  - Custom: Purple
- **Position:** Top-left of message bubble
- **Visual:** Circular avatar with gradient border

**Avatar mapping:**
```typescript
const modelAvatars = {
  'gpt-4': { icon: 'OpenAI', color: 'green' },
  'claude': { icon: 'Anthropic', color: 'orange' },
  'deepseek': { icon: 'DeepSeek', color: 'blue' },
};
```

#### Message Actions Menu
- **Trigger:** Hover over message shows action buttons
- **Actions:**
  - Copy (entire message or code block)
  - Edit (re-send with modifications)
  - Regenerate (retry with same model)
  - Branch (create new conversation from here)
  - Delete
  - Feedback (thumbs up/down)
- **Position:** Top-right corner on hover
- **Visual:** Glass toolbar with icon buttons

```svelte
<!-- MessageActions.svelte -->
<div class="message-actions glass-panel opacity-0 hover:opacity-100">
  <button on:click={copyMessage} title="Copy">
    <CopyIcon />
  </button>
  <button on:click={editMessage} title="Edit">
    <EditIcon />
  </button>
  <button on:click={regenerate} title="Regenerate">
    <RefreshIcon />
  </button>
  <button on:click={branchFrom} title="Branch">
    <TreeIcon />
  </button>
  <button on:click={deleteMessage} title="Delete" class="text-destructive">
    <TrashIcon />
  </button>
</div>
```

#### Message Reactions
- **Feature:** Thumbs up/down for quality feedback
- **Position:** Bottom-left of AI messages
- **Visual:** Small icon buttons with count
- **Data:** Store feedback for analytics

#### Streaming Indicators
- **While streaming:** Animated gradient cursor
- **Character count:** Live update during stream
- **Speed indicator:** chars/second display
- **Visual:** Pulsing gradient dot

**Files to modify:**
- `src/lib/components/chat/Message.svelte` - Add all new UI elements
- `src/lib/components/chat/MessageActions.svelte` - New component
- `src/lib/components/chat/MessageTimestamp.svelte`
- `src/lib/components/chat/ModelAvatar.svelte`
- `src/lib/components/chat/StreamingIndicator.svelte`

---

### 3.2 Better File Handling

**What to implement:**

#### Enhanced Preview Grid
**Current:** 80x80px thumbnails
**New:**
- 120x120px thumbnails with hover zoom
- Glass overlay with file info on hover
- File type badges (PDF, PNG, MP3, etc.)
- Progress bars with gradient
- Drag handles for reordering
- Remove button with confirmation

**Layout:**
```svelte
<div class="file-preview-grid grid grid-cols-4 gap-4">
  {#each files as file}
    <div class="file-preview glass-panel group">
      <!-- Preview thumbnail -->
      <img src={file.preview} alt={file.name} />

      <!-- Hover overlay -->
      <div class="overlay glass-dark opacity-0 group-hover:opacity-100">
        <span class="file-name">{file.name}</span>
        <span class="file-size">{formatBytes(file.size)}</span>
        <button class="remove-btn" on:click={() => removeFile(file.id)}>
          <XIcon />
        </button>
      </div>

      <!-- Type badge -->
      <div class="file-type-badge">{file.type}</div>

      <!-- Progress bar -->
      {#if file.uploading}
        <div class="progress-bar gradient-bg" style="width: {file.progress}%"></div>
      {/if}
    </div>
  {/each}
</div>
```

#### Lightbox for Images
- **Trigger:** Click on image attachment or message
- **Features:**
  - Full-screen overlay with glass background
  - Zoom in/out with mouse wheel
  - Pan with drag
  - Navigation arrows for multiple images
  - Download button
  - Close with ESC or click outside

**Libraries:**
```bash
npm install photoswipe
# or
npm install yet-another-react-lightbox
```

#### PDF Inline Preview
- **Feature:** Show PDF pages inline instead of just icon
- **Layout:** Scrollable page thumbnails
- **Controls:** Page navigation, zoom, download
- **Library:** `pdf.js` or `@react-pdf/renderer`

```svelte
<div class="pdf-preview glass-panel">
  <div class="pdf-toolbar">
    <button on:click={previousPage}>←</button>
    <span>Page {currentPage} / {totalPages}</span>
    <button on:click={nextPage}>→</button>
    <button on:click={downloadPDF}>Download</button>
  </div>
  <canvas bind:this={pdfCanvas}></canvas>
</div>
```

#### Audio Waveform Visualization
- **Feature:** Visual waveform for audio files
- **Playback controls:** Play/pause, seek, volume
- **Visual:** Gradient waveform with glow effect
- **Library:** `wavesurfer.js`

```svelte
<div class="audio-player glass-panel">
  <div class="waveform-container" bind:this={waveformRef}></div>
  <div class="audio-controls">
    <button on:click={togglePlay}>
      {#if playing}
        <PauseIcon />
      {:else}
        <PlayIcon />
      {/if}
    </button>
    <span class="time">{currentTime} / {duration}</span>
    <input type="range" bind:value={volume} min="0" max="100" />
  </div>
</div>
```

#### Drag-to-Reorder
- **Feature:** Rearrange attachments before sending
- **Visual:** Drag handle appears on hover
- **Feedback:** Ghost preview while dragging
- **Library:** `@dnd-kit/core` or native drag-and-drop

**Files to create/modify:**
- `src/lib/components/chat/FilePreviewGrid.svelte` - Enhanced grid
- `src/lib/components/chat/ImageLightbox.svelte` - New component
- `src/lib/components/chat/PDFPreview.svelte` - New component
- `src/lib/components/chat/AudioPlayer.svelte` - New component
- `src/lib/components/chat/FilePreview.svelte` - Individual preview
- `src/lib/utils/fileUtils.ts` - File handling utilities

---

### 3.3 Smart Input Enhancements

**What to implement:**

#### Slash Commands
- **Trigger:** Type `/` in input field
- **Commands:**
  - `/clear` - Clear conversation
  - `/export` - Export conversation
  - `/compare` - Enter comparison mode
  - `/branch` - Create branch
  - `/help` - Show help menu
  - `/model [name]` - Switch model quickly
  - `/prompt [name]` - Apply system prompt

**Implementation:**
```svelte
<script>
  let showCommandMenu = false;
  let commandFilter = '';

  function handleInput(e) {
    const text = e.target.value;
    if (text.startsWith('/')) {
      showCommandMenu = true;
      commandFilter = text.slice(1);
    } else {
      showCommandMenu = false;
    }
  }

  const commands = [
    { name: 'clear', description: 'Clear conversation' },
    { name: 'export', description: 'Export as markdown' },
    // ...
  ];
</script>

{#if showCommandMenu}
  <div class="command-menu glass-panel">
    {#each filteredCommands as cmd}
      <button on:click={() => executeCommand(cmd)}>
        <span class="cmd-name">/{cmd.name}</span>
        <span class="cmd-desc">{cmd.description}</span>
      </button>
    {/each}
  </div>
{/if}
```

#### @ Mentions for System Prompts
- **Trigger:** Type `@` to mention/apply system prompt
- **Visual:** Autocomplete dropdown with prompt library
- **Behavior:** Inserts system prompt at cursor

#### Markdown Formatting Toolbar
- **Location:** Top of textarea on focus
- **Buttons:**
  - Bold (**text**)
  - Italic (*text*)
  - Code (`code`)
  - Code block (```lang)
  - Link ([text](url))
  - List (- item)
  - Heading (# heading)
- **Visual:** Glass toolbar with icon buttons
- **Shortcuts:** Keyboard shortcuts for each

```svelte
<div class="formatting-toolbar glass-panel">
  <button on:click={() => wrapSelection('**', '**')} title="Bold (Cmd+B)">
    <BoldIcon />
  </button>
  <button on:click={() => wrapSelection('*', '*')} title="Italic (Cmd+I)">
    <ItalicIcon />
  </button>
  <button on:click={() => wrapSelection('`', '`')} title="Code (Cmd+E)">
    <CodeIcon />
  </button>
  <!-- ... more buttons -->
</div>
```

#### Auto-save Drafts
- **Feature:** Save input text to localStorage
- **Trigger:** Save every 2 seconds while typing
- **Recovery:** Restore draft on page load
- **Visual:** Small "Draft saved" indicator

```typescript
// Auto-save logic
import { debounce } from 'lodash';

const saveDraft = debounce((text: string) => {
  localStorage.setItem('messageDraft', text);
  showSavedIndicator();
}, 2000);

onMount(() => {
  const draft = localStorage.getItem('messageDraft');
  if (draft) {
    messageInput = draft;
  }
});
```

#### Smart Paste Detection
- **Feature:** Detect content type on paste
- **Types:**
  - Code → Auto-wrap in code block with language detection
  - URL → Show link preview
  - Image → Add as attachment
  - Large text → Ask if should attach as file

**Files to create/modify:**
- `src/lib/components/chat/ChatInput.svelte` - Add all features
- `src/lib/components/chat/CommandMenu.svelte` - Slash commands
- `src/lib/components/chat/FormattingToolbar.svelte` - Toolbar
- `src/lib/components/chat/MentionMenu.svelte` - @ mentions
- `src/lib/utils/draftManager.ts` - Draft saving
- `src/lib/utils/pasteHandler.ts` - Smart paste

---

## Phase 4: Polish & Animations ✨

### 4.1 Micro-interactions

**What to implement:**

#### Spring-based Animations
- **Library:** Svelte's built-in `spring` store or `@react-spring/web`
- **Apply to:**
  - Button press (scale down on click)
  - Modal appearance (scale + fade)
  - Drawer slide (smooth spring physics)
  - Tooltip reveal (bounce in)

```svelte
<script>
  import { spring } from 'svelte/motion';

  const scale = spring(1, { stiffness: 0.1, damping: 0.3 });

  function handlePress() {
    scale.set(0.95);
    setTimeout(() => scale.set(1), 100);
  }
</script>

<button style="transform: scale({$scale})" on:click={handlePress}>
  Click me
</button>
```

#### Stagger Animations for Lists
- **Use case:** Message list, conversation history, file grid
- **Effect:** Items appear one-by-one with slight delay
- **Implementation:**

```svelte
{#each messages as message, i}
  <div
    in:fly={{
      y: 20,
      duration: 300,
      delay: i * 50,
      easing: cubicOut
    }}
  >
    <Message {message} />
  </div>
{/each}
```

#### Loading Skeletons
- **Replace:** Spinner with content-shaped placeholder
- **Visual:** Gradient shimmer animation
- **Apply to:**
  - Message bubbles while streaming
  - Conversation list while loading
  - Model selector while fetching

```svelte
<!-- MessageSkeleton.svelte -->
<div class="message-skeleton glass-panel">
  <div class="skeleton-avatar shimmer"></div>
  <div class="skeleton-content">
    <div class="skeleton-line shimmer" style="width: 80%"></div>
    <div class="skeleton-line shimmer" style="width: 100%"></div>
    <div class="skeleton-line shimmer" style="width: 60%"></div>
  </div>
</div>

<style>
  .shimmer {
    background: linear-gradient(
      90deg,
      rgba(255,255,255,0.05) 0%,
      rgba(255,255,255,0.1) 50%,
      rgba(255,255,255,0.05) 100%
    );
    background-size: 200% 100%;
    animation: shimmer 2s infinite;
  }

  @keyframes shimmer {
    0% { background-position: -200% 0; }
    100% { background-position: 200% 0; }
  }
</style>
```

#### Toast Notifications
- **Library:** `svelte-sonner` or custom implementation
- **Use cases:**
  - File uploaded successfully
  - Message copied
  - Error occurred
  - Draft saved
- **Visual:** Glass panel with icon, auto-dismiss
- **Position:** Top-right corner

```bash
npm install svelte-sonner
```

```svelte
<script>
  import { toast } from 'svelte-sonner';

  function copyMessage() {
    navigator.clipboard.writeText(message.content);
    toast.success('Message copied!', {
      duration: 2000,
      style: 'background: rgba(0,0,0,0.8); backdrop-filter: blur(20px);'
    });
  }
</script>
```

#### Page Transitions
- **Between routes:** Smooth fade + slide
- **Implementation:** SvelteKit transitions

```svelte
<!-- +layout.svelte -->
<script>
  import { page } from '$app/stores';
  import { fade, fly } from 'svelte/transition';
</script>

<div in:fly={{ y: -20, duration: 300 }} out:fade={{ duration: 200 }}>
  <slot />
</div>
```

#### Hover Glow Effects
- **Apply to:**
  - Buttons (glow border on hover)
  - Input fields (glow on focus)
  - Message bubbles (subtle glow on hover)
  - Cards (elevation + glow)

```css
.button-glow {
  position: relative;
  transition: all 0.3s ease;
}

.button-glow::before {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: inherit;
  background: linear-gradient(135deg, #52b788, #06ffa5);
  opacity: 0;
  filter: blur(10px);
  transition: opacity 0.3s ease;
  z-index: -1;
}

.button-glow:hover::before {
  opacity: 0.6;
}
```

**Files to create/modify:**
- `src/lib/components/ui/Skeleton.svelte` - Loading skeletons
- `src/lib/components/ui/Toast.svelte` - Toast notifications
- `src/lib/utils/animations.ts` - Reusable animation configs
- `src/app.css` - Add glow effect utilities

---

### 4.2 Advanced Visual Effects

#### Animated Gradient Backgrounds
- **Where:** Hero sections, empty states
- **Effect:** Slow-moving gradient (5s+ duration)
- **Visual:** Subtle color shifts

```css
.animated-gradient {
  background: linear-gradient(
    135deg,
    #52b788,
    #22d3ee,
    #a855f7,
    #52b788
  );
  background-size: 400% 400%;
  animation: gradientFlow 15s ease infinite;
}

@keyframes gradientFlow {
  0% { background-position: 0% 50%; }
  50% { background-position: 100% 50%; }
  100% { background-position: 0% 50%; }
}
```

#### Particle Effects
- **Use case:** Success states, celebrations
- **Library:** `@tsparticles/svelte` or canvas-based custom
- **Trigger:** First message sent, model comparison complete

#### Smooth Scroll with Momentum
- **Library:** `locomotive-scroll` or CSS `scroll-behavior: smooth`
- **Apply to:** Message container
- **Add:** Scroll shadows at top/bottom edges

```svelte
<div
  class="message-container scroll-smooth"
  bind:this={scrollContainer}
>
  {#each messages as message}
    <Message {message} />
  {/each}

  <!-- Scroll shadows -->
  <div class="scroll-shadow top" class:visible={showTopShadow}></div>
  <div class="scroll-shadow bottom" class:visible={showBottomShadow}></div>
</div>
```

#### Focus Trap with Glow
- **Use case:** Modals, command palette
- **Visual:** Glowing border around focused element
- **Accessibility:** Proper focus management

#### Context Menu
- **Trigger:** Right-click on messages, files
- **Visual:** Glass panel with options
- **Options:** Copy, delete, export, etc.

```svelte
<!-- ContextMenu.svelte -->
<script>
  let x = 0, y = 0, visible = false;

  function handleContextMenu(e) {
    e.preventDefault();
    x = e.clientX;
    y = e.clientY;
    visible = true;
  }
</script>

<div on:contextmenu={handleContextMenu}>
  <!-- Content -->
</div>

{#if visible}
  <div
    class="context-menu glass-panel"
    style="left: {x}px; top: {y}px;"
  >
    <!-- Menu items -->
  </div>
{/if}
```

#### Floating Action Button (FAB)
- **Position:** Bottom-right corner
- **Action:** New conversation
- **Visual:** Circular button with gradient, elevation, hover glow
- **Animation:** Scale + rotate on click

**Files to create/modify:**
- `src/lib/components/ui/FAB.svelte` - Floating action button
- `src/lib/components/ui/ContextMenu.svelte` - Right-click menu
- `src/app.css` - Add animated gradient utilities

---

## Phase 5: Additional Features 🚀

### 5.1 Export & Sharing

**What to implement:**

#### Export as Markdown
```typescript
function exportAsMarkdown(conversation: Conversation): string {
  let markdown = `# ${conversation.name}\n\n`;
  markdown += `**Created:** ${formatDate(conversation.createdAt)}\n`;
  markdown += `**Model:** ${conversation.model}\n\n`;
  markdown += `---\n\n`;

  conversation.messages.forEach(msg => {
    const role = msg.role === 'user' ? 'You' : 'AI';
    markdown += `### ${role}\n\n`;
    markdown += `${msg.content}\n\n`;
  });

  return markdown;
}
```

#### Export as PDF
- **Library:** `jspdf` or `@react-pdf/renderer`
- **Styling:** Maintain code highlighting, formatting
- **Options:** Include/exclude timestamps, system prompts

#### Export as HTML
- **Feature:** Self-contained HTML file with embedded CSS
- **Use case:** Share via email, archive

#### Copy Formatted for Platform
- **Options:**
  - Slack (with formatting)
  - Discord (markdown)
  - Plain text
  - Rich text (for docs)

#### Share Conversation Link
- **Feature:** Generate shareable URL
- **Backend:** Save conversation to database, generate unique ID
- **Privacy:** Option to set expiration, password protection

**Files to create/modify:**
- `src/lib/utils/exporters/markdown.ts`
- `src/lib/utils/exporters/pdf.ts`
- `src/lib/utils/exporters/html.ts`
- `src/lib/components/chat/ExportMenu.svelte`

---

### 5.2 Settings & Preferences

**What to implement:**

#### Theme Customization Panel
- **Location:** Settings page or modal
- **Options:**
  - Accent color picker (green, cyan, purple, custom)
  - Blur intensity slider (10px - 40px)
  - Noise overlay toggle
  - Border style (subtle, normal, prominent)
  - Shadow intensity (low, medium, high)

**Implementation:**
```svelte
<script>
  import { writable } from 'svelte/store';

  export const theme = writable({
    accentColor: '#52b788',
    blurIntensity: 20,
    noiseOverlay: true,
    borderStyle: 'normal',
    shadowIntensity: 'medium',
  });

  // Apply to CSS variables
  $: {
    document.documentElement.style.setProperty('--accent-color', $theme.accentColor);
    document.documentElement.style.setProperty('--blur', `${$theme.blurIntensity}px`);
  }
</script>

<div class="theme-customizer glass-panel">
  <label>
    Accent Color
    <input type="color" bind:value={$theme.accentColor} />
  </label>

  <label>
    Blur Intensity: {$theme.blurIntensity}px
    <input
      type="range"
      min="10"
      max="40"
      bind:value={$theme.blurIntensity}
    />
  </label>

  <!-- More options -->
</div>
```

#### Density Modes
- **Options:**
  - Compact (tight spacing, smaller text)
  - Comfortable (default)
  - Spacious (generous padding, larger text)

**Implementation:**
```css
/* Compact mode */
.density-compact {
  --spacing-unit: 0.5rem;
  --font-size: 0.875rem;
  --message-padding: 0.5rem;
}

/* Comfortable mode */
.density-comfortable {
  --spacing-unit: 1rem;
  --font-size: 1rem;
  --message-padding: 1rem;
}

/* Spacious mode */
.density-spacious {
  --spacing-unit: 1.5rem;
  --font-size: 1.125rem;
  --message-padding: 1.5rem;
}
```

#### Animation Preferences
- **Options:**
  - Full animations (default)
  - Reduced motion (respects `prefers-reduced-motion`)
  - No animations

```svelte
<script>
  import { writable } from 'svelte/store';

  export const animationPreference = writable('full');

  $: animationDuration = {
    full: 300,
    reduced: 150,
    none: 0
  }[$animationPreference];
</script>
```

#### Font Size Scaling
- **Range:** 12px - 18px
- **Applies to:** All text, scales proportionally
- **Visual:** Live preview as you adjust

#### Custom Keyboard Shortcuts
- **Feature:** Rebind shortcuts
- **Library:** `@rwh/keystrokes` or custom implementation
- **UI:** Key recorder (click to record new key combo)

**Files to create/modify:**
- `src/routes/settings/+page.svelte` - Settings page
- `src/lib/stores/preferences.ts` - User preferences store
- `src/lib/components/settings/ThemeCustomizer.svelte`
- `src/lib/components/settings/ShortcutEditor.svelte`

---

## Implementation Priority & Timeline

### Week 1-2: Foundation (Must Have)
- ✅ Glassmorphism design system (1.1)
- ✅ Gradient system (1.2)
- ✅ Shadow system (1.3)
- ✅ Sidebar refresh (1.4 - Sidebar)
- ✅ Message bubble refresh (1.4 - Messages)

### Week 3-4: AI Features (High Value)
- ✅ Token counter (2.1)
- ✅ Cost tracking (2.1)
- ✅ Model comparison mode (2.2)
- ✅ Conversation branching (2.3)

### Week 5: Enhanced Interactions
- ✅ Message timestamps & avatars (3.1)
- ✅ Message actions menu (3.1)
- ✅ Enhanced file previews (3.2)
- ✅ Image lightbox (3.2)

### Week 6: Polish
- ✅ Micro-animations (4.1)
- ✅ Loading skeletons (4.1)
- ✅ Toast notifications (4.1)
- ✅ Hover glow effects (4.1)

### Week 7: Additional Features
- ✅ Export functionality (5.1)
- ✅ Theme customization (5.2)
- ✅ Settings page (5.2)

### Optional (Time Permitting)
- PDF preview (3.2)
- Audio waveforms (3.2)
- Slash commands (3.3)
- Markdown toolbar (3.3)
- Particle effects (4.2)
- Context menu (4.2)

---

## Technical Dependencies

### New NPM Packages to Install
```bash
# Charts for usage dashboard
npm install chart.js svelte-chartjs

# Token counting
npm install tiktoken

# Lightbox for images
npm install photoswipe

# Toast notifications
npm install svelte-sonner

# Date utilities (already have date-fns)
# npm install date-fns

# Drag and drop (if using library)
npm install @dnd-kit/core @dnd-kit/sortable

# PDF handling (optional)
npm install pdfjs-dist

# Audio waveforms (optional)
npm install wavesurfer.js
```

### Tailwind Config Extensions
```javascript
// tailwind.config.js additions
module.exports = {
  theme: {
    extend: {
      backdropBlur: {
        xs: '2px',
        '3xl': '40px',
      },
      boxShadow: {
        'low': '0 2px 8px rgba(0, 0, 0, 0.1)',
        'medium': '0 4px 16px rgba(0, 0, 0, 0.15)',
        'high': '0 8px 32px rgba(0, 0, 0, 0.2)',
        'float': '0 16px 48px rgba(0, 0, 0, 0.3)',
        'glow-green': '0 0 20px rgba(82, 183, 136, 0.5)',
        'glow-cyan': '0 0 20px rgba(34, 211, 238, 0.5)',
      },
      animation: {
        'gradient': 'gradientFlow 15s ease infinite',
        'shimmer': 'shimmer 2s infinite',
      },
      keyframes: {
        gradientFlow: {
          '0%, 100%': { backgroundPosition: '0% 50%' },
          '50%': { backgroundPosition: '100% 50%' },
        },
        shimmer: {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
      },
    },
  },
  plugins: [],
};
```

---

## File Structure Overview

New files to create:
```
src/
├── lib/
│   ├── components/
│   │   ├── chat/
│   │   │   ├── TokenCounter.svelte (new)
│   │   │   ├── CostEstimator.svelte (new)
│   │   │   ├── MessageActions.svelte (new)
│   │   │   ├── MessageTimestamp.svelte (new)
│   │   │   ├── ModelAvatar.svelte (new)
│   │   │   ├── BranchIndicator.svelte (new)
│   │   │   ├── BranchSwitcher.svelte (new)
│   │   │   ├── FilePreviewGrid.svelte (new)
│   │   │   ├── ImageLightbox.svelte (new)
│   │   │   ├── CommandMenu.svelte (new)
│   │   │   └── FormattingToolbar.svelte (new)
│   │   ├── comparison/
│   │   │   ├── ComparisonGrid.svelte (new)
│   │   │   ├── ModelColumn.svelte (new)
│   │   │   └── MetricsDisplay.svelte (new)
│   │   ├── settings/
│   │   │   ├── ThemeCustomizer.svelte (new)
│   │   │   └── ShortcutEditor.svelte (new)
│   │   └── ui/
│   │       ├── Skeleton.svelte (new)
│   │       ├── FAB.svelte (new)
│   │       └── ContextMenu.svelte (new)
│   ├── stores/
│   │   ├── tokenUsage.ts (new)
│   │   ├── branches.ts (new)
│   │   └── preferences.ts (new)
│   └── utils/
│       ├── tokenCalculator.ts (new)
│       ├── costCalculator.ts (new)
│       ├── branchManager.ts (new)
│       ├── exporters/
│       │   ├── markdown.ts (new)
│       │   ├── pdf.ts (new)
│       │   └── html.ts (new)
│       └── animations.ts (new)
└── routes/
    ├── usage/
    │   └── +page.svelte (new)
    ├── compare/
    │   └── +page.svelte (new)
    └── settings/
        └── +page.svelte (new)
```

---

## Next Steps

1. **Review this plan** - Make sure you're happy with the scope
2. **Prioritize features** - Pick what's most important to you
3. **Start with Phase 1** - Visual foundation is critical
4. **Iterate** - Build, test, refine, repeat
5. **Gather feedback** - Test with users as you build

Ready to transform your AI chat into a stunning, modern application! Let me know which phase you'd like to tackle first, and we'll dive in together. 🚀
