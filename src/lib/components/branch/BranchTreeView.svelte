<script lang="ts">
	import { onMount } from 'svelte';
	import { branchStore } from '$lib/stores/branches';
	import { branchManager } from '$lib/utils/branchManager';
	import { treeLayout } from '$lib/utils/treeLayout';
	import TreeNode from './TreeNode.svelte';
	import type { ConversationTree } from '$lib/types';

	let { conversationId }: { conversationId: string } = $props();

	let tree = $derived($branchStore.tree);
	let selectedPath = $derived($branchStore.selectedPath);

	let svgWidth = $state(800);
	let svgHeight = $state(600);
	let roots = $state<any[]>([]);
	let paths = $state<any[]>([]);

	// Build and layout the tree when tree data changes
	$effect(() => {
		if (tree) {
			buildTreeLayout(tree);
		}
	});

	function buildTreeLayout(treeData: ConversationTree) {
		// Build tree structure
		const treeRoots = branchManager.buildTree(treeData);

		// Calculate layout
		const layout = treeLayout.layout(treeRoots);
		roots = layout.nodes;

		// Generate connection paths
		paths = treeLayout.generatePaths(roots);

		// Update SVG dimensions
		svgWidth = Math.max(layout.width + 100, 800);
		svgHeight = Math.max(layout.height + 100, 600);
	}

	function handleNodeClick(messageId: string) {
		console.log('Node clicked:', messageId);
		// TODO: Navigate to this message in the conversation
	}

	function isNodeSelected(messageId: string): boolean {
		return selectedPath.includes(messageId);
	}

	// Get color for path based on branch
	function getPathColor(fromId: string): string {
		const node = roots.find((n) => n.messageId === fromId);
		if (!node) return '#52b788';

		const colors: Record<string, string> = {
			green: '#52b788',
			cyan: '#22d3ee',
			purple: '#a855f7',
			amber: '#fbbf24'
		};

		const hash = node.branchId.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
		const colorKeys = Object.keys(colors);
		return colors[colorKeys[hash % colorKeys.length]];
	}
</script>

<div class="tree-view-container">
	{#if tree && roots.length > 0}
		<div class="tree-canvas">
			<svg width={svgWidth} height={svgHeight} class="tree-svg">
				<!-- Define gradients for paths -->
				<defs>
					<linearGradient id="pathGradient" x1="0%" y1="0%" x2="0%" y2="100%">
						<stop offset="0%" stop-color="#52b788" stop-opacity="0.6" />
						<stop offset="100%" stop-color="#52b788" stop-opacity="0.3" />
					</linearGradient>
				</defs>

				<!-- Draw connection paths -->
				<g class="paths">
					{#each paths as path}
						<path
							d={path.path}
							fill="none"
							stroke={getPathColor(path.from)}
							stroke-width="2"
							opacity="0.4"
							class="connection-path"
						/>
					{/each}
				</g>

				<!-- Draw nodes -->
				<g class="nodes">
					{#each roots as node (node.messageId)}
						<TreeNode {node} isSelected={isNodeSelected(node.messageId)} onClick={handleNodeClick} />
					{/each}
				</g>
			</svg>
		</div>
	{:else}
		<div class="empty-state">
			<p>No branch tree to display</p>
			<p class="hint">Create branches from messages to see the tree structure</p>
		</div>
	{/if}
</div>

<style>
	.tree-view-container {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		background: rgba(0, 0, 0, 0.2);
		border-radius: 0.5rem;
		overflow: hidden;
	}

	.tree-canvas {
		flex: 1;
		overflow: auto;
		padding: 2rem;
	}

	.tree-svg {
		display: block;
	}

	.connection-path {
		transition: all 0.2s ease;
	}

	.connection-path:hover {
		stroke-width: 3;
		opacity: 0.6;
	}

	.empty-state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		padding: 3rem;
		text-align: center;
	}

	.empty-state p {
		color: rgba(255, 255, 255, 0.6);
		font-size: 0.875rem;
	}

	.empty-state .hint {
		font-size: 0.75rem;
		color: rgba(255, 255, 255, 0.4);
	}
</style>
