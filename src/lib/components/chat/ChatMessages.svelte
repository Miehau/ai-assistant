<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import ChatMessage from "../ChatMessage.svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import type { Message, ToolCallRecord } from "$lib/types";
  import { streamingMessage, isStreaming, streamingSegmentedLength } from "$lib/stores/chat";
  import { pageVisible } from "$lib/stores/visibility";
  import ToolApprovalQueue from "./ToolApprovalQueue.svelte";
  import ToolCallGroup from "./ToolCallGroup.svelte";
  import SubagentExecutionGroup from "./SubagentExecutionGroup.svelte";
  import WorkflowExecutionGroup from "./WorkflowExecutionGroup.svelte";
  import type { ToolExecutionProposedPayload } from "$lib/types/events";
  import { getPhaseLabel } from "$lib/types/agent";
  import type { AgentPlan, AgentPlanStep, PhaseKind } from "$lib/types/agent";
  import type { ToolActivityEntry } from "$lib/stores/chat";
  import { groupToolCallsBySession, flattenToolCallGroups, computeDisplayItems } from "$lib/utils/toolCallGrouping";

  export let messages: Message[] = [];
  export let chatContainer: HTMLElement | null = null;
  export let autoScroll = true;
  export let conversationId: string | undefined = undefined;
  export let toolApprovals: ToolExecutionProposedPayload[] = [];
  export let toolActivity: ToolActivityEntry[] = [];
  export let agentPhase: PhaseKind | null = null;
  export let agentPlan: AgentPlan | null = null;
  export let agentPlanSteps: AgentPlanStep[] = [];
  export let isLoading = false;

  const INITIAL_VISIBLE_MESSAGES = 60;
  const LOAD_MORE_CHUNK = 40;
  const LOAD_MORE_THRESHOLD = 120;
  const ANIMATED_MESSAGE_LIMIT = 12;

  $: {
    // Keep optional props referenced to avoid unused export warnings.
    void toolActivity;
    void agentPlan;
    void agentPlanSteps;
  }

  $: phaseLabel = getPhaseLabel(agentPhase);
  // Only the text after the last flushed text segment — shown in the streaming div.
  // Pre-tool text is already rendered inline via msg.segments, so showing the full
  // $streamingMessage would duplicate it.
  $: streamingTail = $streamingMessage.slice($streamingSegmentedLength);
  $: showThinkingStatus =
    (isLoading || $isStreaming) &&
    !($isStreaming && streamingTail.length > 0);

  function shouldRenderMessage(msg: Message): boolean {
    if (msg.type === "sent") return true;
    if (msg.content.trim().length > 0) return true;
    return Boolean(msg.attachments && msg.attachments.length > 0);
  }

  let lastScrollHeight = 0;
  let lastScrollTop = 0;
  let lastMessageCount = 0;
  let scrollThrottleTimeout: number | null = null;
  let resizeThrottleTimeout: number | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let visibleCount = INITIAL_VISIBLE_MESSAGES;
  let visibleMessages: Message[] = [];
  let hasMoreMessages = false;
  let loadingMore = false;
  let lastTotalCount = 0;
  const BOTTOM_FOLLOW_THRESHOLD = 24;
  let preservingPrependedHistory = false;

  function isNearBottom(): boolean {
    if (!chatContainer) return true;
    const distanceFromBottom =
      chatContainer.scrollHeight - (chatContainer.scrollTop + chatContainer.clientHeight);
    return distanceFromBottom <= BOTTOM_FOLLOW_THRESHOLD;
  }

  function preserveScrollFromBottom() {
    if (!chatContainer) return;

    const newScrollHeight = chatContainer.scrollHeight;
    const visibleHeight = chatContainer.clientHeight;

    // Calculate distance from bottom before resize
    const distanceFromBottom =
      lastScrollHeight - (lastScrollTop + visibleHeight);

    // Restore the same distance from bottom after resize
    chatContainer.scrollTop =
      newScrollHeight - (distanceFromBottom + visibleHeight);

    // Update values for next resize
    lastScrollHeight = newScrollHeight;
    lastScrollTop = chatContainer.scrollTop;
  }

  // Throttled version to reduce frequency of scroll preservation
  function throttledPreserveScroll() {
    if (resizeThrottleTimeout !== null) {
      return; // Skip if already scheduled
    }

    resizeThrottleTimeout = window.setTimeout(() => {
      preserveScrollFromBottom();
      resizeThrottleTimeout = null;
    }, 100) as unknown as number; // Throttle to max 10 times per second
  }

  function handleScroll() {
    if (!chatContainer) return;
    autoScroll = isNearBottom();
    lastScrollHeight = chatContainer.scrollHeight;
    lastScrollTop = chatContainer.scrollTop;

    if (
      chatContainer.scrollTop <= LOAD_MORE_THRESHOLD &&
      hasMoreMessages &&
      !loadingMore
    ) {
      loadMoreMessages();
    }
  }

  // Setup event listeners and observers
  function setupScrollBehavior() {
    if (!chatContainer) return;

    // Setup scroll event listener
    chatContainer.addEventListener('scroll', handleScroll);

    // Setup resize observer
    resizeObserver = new ResizeObserver(() => {
      if (autoScroll) {
        throttledScrollToBottom();
      } else if (preservingPrependedHistory) {
        throttledPreserveScroll();
      }
    });
    resizeObserver.observe(chatContainer);

    lastScrollHeight = chatContainer.scrollHeight;
    lastScrollTop = chatContainer.scrollTop;
    autoScroll = isNearBottom();
  }

  // Cleanup function
  function cleanup() {
    if (chatContainer) {
      chatContainer.removeEventListener('scroll', handleScroll);
    }
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }
    if (scrollThrottleTimeout !== null) {
      clearTimeout(scrollThrottleTimeout);
      scrollThrottleTimeout = null;
    }
    if (resizeThrottleTimeout !== null) {
      clearTimeout(resizeThrottleTimeout);
      resizeThrottleTimeout = null;
    }
  }

  // Initialize on mount
  onMount(() => {
    setupScrollBehavior();
  });

  onDestroy(() => {
    cleanup();
  });

  export function scrollToBottom() {
    if (chatContainer && autoScroll) {
      const newScrollTop = chatContainer.scrollHeight - chatContainer.clientHeight;
      chatContainer.scrollTop = newScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = newScrollTop;
    }
  }

  // Throttled scroll to bottom to reduce DOM thrashing
  // Use longer throttle during streaming to reduce performance impact
  function throttledScrollToBottom() {
    if (scrollThrottleTimeout !== null) {
      return; // Skip if already scheduled
    }

    scrollThrottleTimeout = window.setTimeout(() => {
      scrollToBottom();
      scrollThrottleTimeout = null;
    }, 100) as unknown as number; // Increased from 16ms to 100ms to reduce frequency
  }

  // Track last streaming message length to avoid triggering on every chunk
  let lastStreamingLength = 0;

  function loadMoreMessages() {
    if (!chatContainer || loadingMore) return;
    loadingMore = true;
    preservingPrependedHistory = true;

    const prevScrollHeight = chatContainer.scrollHeight;
    const prevScrollTop = chatContainer.scrollTop;

    visibleCount = Math.min(messages.length, visibleCount + LOAD_MORE_CHUNK);

    requestAnimationFrame(() => {
      if (!chatContainer) {
        loadingMore = false;
        return;
      }
      const newScrollHeight = chatContainer.scrollHeight;
      chatContainer.scrollTop = newScrollHeight - prevScrollHeight + prevScrollTop;
      lastScrollHeight = chatContainer.scrollHeight;
      lastScrollTop = chatContainer.scrollTop;
      loadingMore = false;
      preservingPrependedHistory = false;
    });
  }

  $: {
    // Collect all subagent tool calls
    const allSubagentCalls: typeof messages[0]['tool_calls'] = [];
    const subagentMessageIds = new Set<string>();

    for (const msg of messages) {
      if (msg.tool_calls) {
        const allAreSubagent = msg.tool_calls.every(call => call.is_sub_agent);
        if (allAreSubagent && msg.tool_calls.length > 0 && !msg.content?.trim()) {
          // This is a subagent-only message
          subagentMessageIds.add(msg.id);
          allSubagentCalls.push(...msg.tool_calls);
        }
      }
    }

    const subagentCallsByParentSession = new Map<string, NonNullable<typeof messages[0]['tool_calls']>>();
    for (const call of allSubagentCalls ?? []) {
      if (!call.parent_session_id) continue;
      const existing = subagentCallsByParentSession.get(call.parent_session_id) ?? [];
      existing.push(call);
      subagentCallsByParentSession.set(call.parent_session_id, existing);
    }

    function collectDescendantSubagentCalls(
      parentSessionId: string,
      seen = new Set<string>()
    ): NonNullable<typeof messages[0]['tool_calls']> {
      if (seen.has(parentSessionId)) return [];
      seen.add(parentSessionId);

      const directCalls = subagentCallsByParentSession.get(parentSessionId) ?? [];
      const collected: NonNullable<typeof messages[0]['tool_calls']> = [];
      for (const call of directCalls) {
        collected.push(call);
        if (call.session_id) {
          collected.push(...collectDescendantSubagentCalls(call.session_id, seen));
        }
      }
      return collected;
    }

    // Find parent messages (messages with delegate/agent.spawn) and merge subagent calls
    const mergedMessages = messages
      .filter(msg => !subagentMessageIds.has(msg.id))
      .map(msg => {
        if (!msg.tool_calls) return msg;

        // Check if this message has a delegate/agent.spawn call
        const hasDelegate = msg.tool_calls.some(
          call => call.tool_name === 'delegate' || call.tool_name === 'agent.spawn'
        );
        if (!hasDelegate) return msg;

        // Get session_id from the message's tool calls
        const sessionId = msg.tool_calls.find(call => call.session_id)?.session_id;
        if (!sessionId) return msg;

        // Filter subagent calls that belong to this parent session
        const relevantSubagentCalls = collectDescendantSubagentCalls(sessionId);

        if (relevantSubagentCalls.length === 0) return msg;

        return {
          ...msg,
          tool_calls: [...msg.tool_calls, ...relevantSubagentCalls]
        };
      })
      .sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));

    const total = mergedMessages.length;
    if (total < lastTotalCount) {
      visibleCount = INITIAL_VISIBLE_MESSAGES;
    } else if (total > lastTotalCount) {
      visibleCount = Math.min(total, Math.max(visibleCount, INITIAL_VISIBLE_MESSAGES));
    }
    lastTotalCount = total;

    const startIndex = Math.max(0, total - visibleCount);
    visibleMessages = mergedMessages.slice(startIndex);
    hasMoreMessages = startIndex > 0;
  }

  // Only scroll when messages actually change or streaming updates
  $: if (messages.length !== lastMessageCount || ($streamingMessage && $streamingMessage.length !== lastStreamingLength)) {
    lastMessageCount = messages.length;
    lastStreamingLength = $streamingMessage.length;
    requestAnimationFrame(() => {
      if (autoScroll) {
        throttledScrollToBottom();
      }
    });
  }

  // Handle visibility changes - scroll to bottom when page becomes visible
  // This ensures proper scroll position after deferred markdown parsing completes
  $: if ($pageVisible && messages.length > 0 && autoScroll) {
    // Scroll immediately for instant feedback
    requestAnimationFrame(() => {
      throttledScrollToBottom();

      // Then wait for deferred content to render and scroll again
      // This catches any height changes from requestIdleCallback parsing in ChatMessage
      if ('requestIdleCallback' in window) {
        requestIdleCallback(() => {
          if (autoScroll) {
            throttledScrollToBottom();
          }
        });
      } else {
        setTimeout(() => {
          if (autoScroll) {
            throttledScrollToBottom();
          }
        }, 100);
      }
    });
  }
</script>

<div
  bind:this={chatContainer}
  class="h-full overflow-y-auto pr-4 space-y-4 w-full min-w-0 px-2 md:px-4 lg:px-6"
>
  {#if hasMoreMessages}
    <div class="flex justify-center">
      <button
        class="text-xs text-muted-foreground/80 px-3 py-1 rounded-full border border-white/10 hover:bg-white/5 transition-all"
        onclick={loadMoreMessages}
        disabled={loadingMore}
      >
        {loadingMore ? "Loading..." : "Load earlier messages"}
      </button>
    </div>
  {/if}

  {#each visibleMessages as msg, i (msg.id || `${msg.type}-${i}`)}
    {@const animated = i >= visibleMessages.length - ANIMATED_MESSAGE_LIMIT}
    {@const toolsSettled = !isLoading && !$isStreaming && (msg.tool_calls ?? []).length > 0 && (msg.tool_calls ?? []).every(c => c.success !== undefined)}
    {#if msg.segments && msg.segments.length > 0}
      <!-- Interleaved rendering: text and tool calls in streaming order.
           computeDisplayItems batches consecutive non-subagent tool anchors so they
           render as a single collapsible ToolCallGroup instead of N separate bubbles. -->
      {@const toolGroups = groupToolCallsBySession(msg.tool_calls ?? [])}
      {@const allToolGroups = flattenToolCallGroups(toolGroups)}
      {@const topLevelGroups = allToolGroups.filter((g) => !g.parentSubagentSessionId)}
      {@const execToGroup = new Map(topLevelGroups
        .filter((g) => !g.isSubAgent || !g.spawnCall)
        .flatMap((g) => g.calls.map((c) => [c.execution_id, g])))}
      {@const toolSegmentIds = msg.segments.filter((s) => s.kind === 'tool').map((s) => s.execution_id)}
      {@const spawnExecToSubagentGroup = new Map(
        topLevelGroups
          .filter((g) => g.isSubAgent && g.spawnCall)
          .map((g) => [g.spawnCall!.execution_id, g])
      )}
      {@const anchorExecIds = new Set([
        ...topLevelGroups.filter((g) => !g.isSubAgent || !g.spawnCall).map((g) => {
          const groupExecIds = new Set(g.calls.map((c) => c.execution_id));
          return toolSegmentIds.find((id) => groupExecIds.has(id));
        }).filter(Boolean),
        ...Array.from(spawnExecToSubagentGroup.keys()).filter((id) => toolSegmentIds.includes(id)),
      ])}
      {@const displayItems = computeDisplayItems(msg.segments, anchorExecIds, execToGroup, spawnExecToSubagentGroup)}
      {#each displayItems as item, di (`${i}-${di}`)}
        {#if item.kind === 'text'}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              out:scale={{ duration: 100, start: 0.98, opacity: 0 }}
              class="w-full message-container"
            >
              <ChatMessage
                type={msg.type}
                content={item.content}
                attachments={msg.attachments}
                messageId={msg.id}
                conversationId={conversationId}
                agentActivity={msg.agentActivity}
                isError={msg.isError}
              />
            </div>
          {:else}
            <div class="w-full message-container">
              <ChatMessage
                type={msg.type}
                content={item.content}
                attachments={msg.attachments}
                messageId={msg.id}
                conversationId={conversationId}
                agentActivity={msg.agentActivity}
                isError={msg.isError}
              />
            </div>
          {/if}
        {:else if item.kind === 'tool-batch'}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              class="w-full message-container"
            >
              <ToolCallGroup calls={item.calls} settled={toolsSettled} />
            </div>
          {:else}
            <div class="w-full message-container">
              <ToolCallGroup calls={item.calls} settled={toolsSettled} />
            </div>
          {/if}
        {:else if item.kind === 'subagent'}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              class="w-full message-container"
            >
              <SubagentExecutionGroup calls={item.group.calls} sessionId={item.group.sessionId} spawnCall={item.group.spawnCall} childGroups={item.group.childGroups ?? []} />
            </div>
          {:else}
            <div class="w-full message-container">
              <SubagentExecutionGroup calls={item.group.calls} sessionId={item.group.sessionId} spawnCall={item.group.spawnCall} childGroups={item.group.childGroups ?? []} />
            </div>
          {/if}
        {:else if item.kind === 'workflow'}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              class="w-full message-container"
            >
              <WorkflowExecutionGroup call={item.call} />
            </div>
          {:else}
            <div class="w-full message-container">
              <WorkflowExecutionGroup call={item.call} />
            </div>
          {/if}
        {/if}
      {/each}
    {:else}
      <!-- Legacy rendering: tool calls first, then message text.
           All non-subagent calls from one message go into a single ToolCallGroup. -->
      {#if msg.type === "received" && msg.tool_calls && msg.tool_calls.length > 0}
        {@const toolGroups = groupToolCallsBySession(msg.tool_calls)}
        {@const subagentSpawnExecIds = new Set(toolGroups.filter(g => g.isSubAgent && g.spawnCall).map(g => g.spawnCall!.execution_id))}
        {@const allMainCalls = toolGroups.filter(g => !g.isSubAgent && !subagentSpawnExecIds.has(g.calls[0]?.execution_id)).flatMap(g => g.calls)}
        {@const mainCalls = allMainCalls.filter(c => c.tool_name !== 'workflow.run')}
        {@const workflowCalls = allMainCalls.filter(c => c.tool_name === 'workflow.run')}
        {@const subagentGroups = toolGroups.filter(g => g.isSubAgent)}
        {#each workflowCalls as wfCall (wfCall.execution_id)}
          <div class="w-full message-container">
            <WorkflowExecutionGroup call={wfCall} />
          </div>
        {/each}
        {#if mainCalls.length > 0}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              class="w-full message-container"
            >
              <ToolCallGroup calls={mainCalls} settled={toolsSettled} />
            </div>
          {:else}
            <div class="w-full message-container">
              <ToolCallGroup calls={mainCalls} settled={toolsSettled} />
            </div>
          {/if}
        {/if}
        {#each subagentGroups as group (group.sessionId)}
          {#if animated}
            <div
              in:fly={{ y: 10, duration: 150, easing: backOut }}
              class="w-full message-container"
            >
              <SubagentExecutionGroup calls={group.calls} sessionId={group.sessionId} spawnCall={group.spawnCall} childGroups={group.childGroups ?? []} />
            </div>
          {:else}
            <div class="w-full message-container">
              <SubagentExecutionGroup calls={group.calls} sessionId={group.sessionId} spawnCall={group.spawnCall} childGroups={group.childGroups ?? []} />
            </div>
          {/if}
        {/each}
      {/if}
      {#if shouldRenderMessage(msg)}
        {#if animated}
          <div
            in:fly={{ y: 10, duration: 150, easing: backOut }}
            out:scale={{ duration: 100, start: 0.98, opacity: 0 }}
            class="w-full message-container"
          >
            <ChatMessage
              type={msg.type}
              content={msg.content}
              attachments={msg.attachments}
              messageId={msg.id}
              conversationId={conversationId}
              agentActivity={msg.agentActivity}
              isError={msg.isError}
            />
          </div>
        {:else}
          <div class="w-full message-container">
            <ChatMessage
              type={msg.type}
              content={msg.content}
              attachments={msg.attachments}
              messageId={msg.id}
              conversationId={conversationId}
              agentActivity={msg.agentActivity}
              isError={msg.isError}
            />
          </div>
        {/if}
      {/if}
    {/if}
  {/each}

  {#if toolApprovals.length > 0}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container flex justify-start"
    >
      <ToolApprovalQueue approvals={toolApprovals} containerClass="w-full max-w-5xl min-w-0 flex-1" />
    </div>
  {/if}

  {#if showThinkingStatus}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container flex justify-start"
    >
      <div class="rounded-2xl px-4 py-2 w-full max-w-5xl min-w-0 bg-background/50 border border-border/60">
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <span class="thinking-orb" aria-hidden="true"></span>
          <span>Thinking</span>
          <span class="thinking-dots" aria-hidden="true">
            <span>.</span><span>.</span><span>.</span>
          </span>
          {#if agentPhase !== null && phaseLabel !== "Idle"}
            <span class="text-foreground/80"> {phaseLabel}</span>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Streaming tail: only the text since the last tool boundary.
       Pre-tool text is already rendered inline via msg.segments. -->
  {#if $isStreaming && streamingTail}
    <div
      in:fly={{ y: 10, duration: 150, easing: backOut }}
      class="w-full message-container"
    >
      <ChatMessage
        type="received"
        content={streamingTail}
        conversationId={conversationId}
        isStreaming={true}
      />
    </div>
  {/if}
</div>

<style>
  /* Add to existing styles */
  :global(.message-container) {
    transform-origin: center;
  }

  @keyframes dustAway {
    0% {
      opacity: 1;
      transform: translateX(0) rotate(0);
    }
    50% {
      opacity: 0.5;
      transform: translateX(20px) rotate(5deg) scale(0.95);
    }
    100% {
      opacity: 0;
      transform: translateX(40px) rotate(10deg) scale(0.9);
    }
  }

  .thinking-orb {
    width: 6px;
    height: 6px;
    border-radius: 9999px;
    background: rgba(16, 185, 129, 0.75);
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.55);
    animation: orbPulse 1.2s ease-in-out infinite;
  }

  .thinking-dots span {
    display: inline-block;
    margin-left: 2px;
    animation: dotBlink 1.2s ease-in-out infinite;
  }

  .thinking-dots span:nth-child(2) {
    animation-delay: 0.2s;
  }

  .thinking-dots span:nth-child(3) {
    animation-delay: 0.4s;
  }

  @keyframes orbPulse {
    0%,
    100% {
      transform: scale(0.9);
      opacity: 0.6;
    }
    50% {
      transform: scale(1.15);
      opacity: 1;
    }
  }

  @keyframes dotBlink {
    0%,
    100% {
      opacity: 0.2;
    }
    50% {
      opacity: 1;
    }
  }
</style>
