<script lang="ts">
	import { Circle, MessageSquare } from 'lucide-svelte';
	import type { TreeNode } from '$lib/utils/branchManager';

	let {
		node,
		isSelected = false,
		onClick
	}: {
		node: TreeNode;
		isSelected?: boolean;
		onClick?: (messageId: string) => void;
	} = $props();

	function handleClick() {
		onClick?.(node.messageId);
	}

	// Get branch color based on branch ID (simple hash)
	function getBranchColor(branchId: string): string {
		const colors = ['#52b788', '#22d3ee', '#a855f7', '#fbbf24'];
		const hash = branchId.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
		return colors[hash % colors.length];
	}

	const branchColor = $derived(getBranchColor(node.branchId));
</script>

<g
	class="tree-node"
	class:selected={isSelected}
	class:branch-point={node.isBranchPoint}
	transform="translate({node.x}, {node.y})"
	onclick={handleClick}
	style="cursor: pointer;"
>
	<!-- Node rectangle -->
	<rect
		x="0"
		y="0"
		width="100"
		height="50"
		rx="8"
		class="node-bg"
		fill="rgba(255, 255, 255, 0.05)"
		stroke={branchColor}
		stroke-width={isSelected ? 2 : 1}
		opacity={isSelected ? 1 : 0.8}
	/>

	<!-- Glow effect when selected -->
	{#if isSelected}
		<rect
			x="-2"
			y="-2"
			width="104"
			height="54"
			rx="10"
			fill="none"
			stroke={branchColor}
			stroke-width="2"
			opacity="0.3"
			class="glow"
		/>
	{/if}

	<!-- Icon -->
	<foreignObject x="8" y="12" width="24" height="24">
		<div class="node-icon" style="color: {branchColor}">
			{#if node.isBranchPoint}
				<Circle size={18} fill={branchColor} />
			{:else}
				<MessageSquare size={18} />
			{/if}
		</div>
	</foreignObject>

	<!-- Message ID (truncated) -->
	<text x="38" y="30" class="node-text" fill="rgba(255, 255, 255, 0.9)" font-size="11">
		{node.messageId.substring(0, 8)}...
	</text>
</g>

<style>
	.tree-node {
		transition: all 0.2s ease;
	}

	.tree-node:hover .node-bg {
		fill: rgba(255, 255, 255, 0.1);
		stroke-width: 2;
	}

	.tree-node.selected .node-bg {
		fill: rgba(255, 255, 255, 0.15);
	}

	.node-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 100%;
	}

	.node-text {
		font-family: 'SF Mono', 'Menlo', 'Monaco', 'Courier New', monospace;
		font-weight: 500;
	}

	.glow {
		filter: blur(8px);
		animation: pulse 2s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 0.2;
		}
		50% {
			opacity: 0.4;
		}
	}
</style>
