import type { TreeNode } from './branchManager';

/**
 * Configuration for tree layout
 */
export interface LayoutConfig {
	nodeWidth: number;
	nodeHeight: number;
	horizontalSpacing: number;
	verticalSpacing: number;
}

const DEFAULT_CONFIG: LayoutConfig = {
	nodeWidth: 120,
	nodeHeight: 60,
	horizontalSpacing: 40,
	verticalSpacing: 100
};

/**
 * Tree layout algorithm for SVG visualization
 * Uses a simplified version of the Reingold-Tilford algorithm
 */
export class TreeLayout {
	private config: LayoutConfig;

	constructor(config: Partial<LayoutConfig> = {}) {
		this.config = { ...DEFAULT_CONFIG, ...config };
	}

	/**
	 * Calculate positions for all nodes in the tree
	 */
	layout(roots: TreeNode[]): { nodes: TreeNode[]; width: number; height: number } {
		// First pass: assign initial x positions based on tree structure
		let currentX = 0;
		roots.forEach((root) => {
			currentX = this.assignInitialX(root, currentX);
		});

		// Second pass: calculate final positions
		const allNodes: TreeNode[] = [];
		roots.forEach((root) => {
			this.calculatePositions(root, 0, allNodes);
		});

		// Calculate bounds
		const bounds = this.calculateBounds(allNodes);

		return {
			nodes: allNodes,
			width: bounds.maxX - bounds.minX + this.config.nodeWidth,
			height: bounds.maxY + this.config.nodeHeight
		};
	}

	/**
	 * Assign initial X positions based on tree structure
	 */
	private assignInitialX(node: TreeNode, startX: number): number {
		if (node.children.length === 0) {
			node.x = startX;
			return startX + this.config.nodeWidth + this.config.horizontalSpacing;
		}

		// Recursively layout children
		let currentX = startX;
		node.children.forEach((child) => {
			currentX = this.assignInitialX(child, currentX);
		});

		// Position parent at the center of its children
		const firstChild = node.children[0];
		const lastChild = node.children[node.children.length - 1];
		node.x = (firstChild.x + lastChild.x) / 2;

		return currentX;
	}

	/**
	 * Calculate final positions with proper spacing
	 */
	private calculatePositions(node: TreeNode, depth: number, allNodes: TreeNode[]): void {
		node.y = depth * (this.config.nodeHeight + this.config.verticalSpacing);
		allNodes.push(node);

		node.children.forEach((child) => {
			this.calculatePositions(child, depth + 1, allNodes);
		});
	}

	/**
	 * Calculate bounding box of the tree
	 */
	private calculateBounds(nodes: TreeNode[]): {
		minX: number;
		maxX: number;
		minY: number;
		maxY: number;
	} {
		if (nodes.length === 0) {
			return { minX: 0, maxX: 0, minY: 0, maxY: 0 };
		}

		const xs = nodes.map((n) => n.x);
		const ys = nodes.map((n) => n.y);

		return {
			minX: Math.min(...xs),
			maxX: Math.max(...xs),
			minY: Math.min(...ys),
			maxY: Math.max(...ys)
		};
	}

	/**
	 * Generate SVG path between two nodes (parent and child)
	 */
	generatePath(parent: TreeNode, child: TreeNode): string {
		const startX = parent.x + this.config.nodeWidth / 2;
		const startY = parent.y + this.config.nodeHeight;
		const endX = child.x + this.config.nodeWidth / 2;
		const endY = child.y;

		// Use cubic bezier curve for smooth connections
		const midY = (startY + endY) / 2;

		return `M ${startX},${startY} C ${startX},${midY} ${endX},${midY} ${endX},${endY}`;
	}

	/**
	 * Generate connection paths for entire tree
	 */
	generatePaths(nodes: TreeNode[]): Array<{ from: string; to: string; path: string }> {
		const paths: Array<{ from: string; to: string; path: string }> = [];

		nodes.forEach((node) => {
			node.children.forEach((child) => {
				paths.push({
					from: node.messageId,
					to: child.messageId,
					path: this.generatePath(node, child)
				});
			});
		});

		return paths;
	}

	/**
	 * Calculate compact horizontal layout (for simple linear branches)
	 */
	layoutHorizontal(nodes: TreeNode[]): { nodes: TreeNode[]; width: number; height: number } {
		nodes.forEach((node, index) => {
			node.x = index * (this.config.nodeWidth + this.config.horizontalSpacing);
			node.y = node.depth * (this.config.nodeHeight + this.config.verticalSpacing);
		});

		const width =
			nodes.length > 0
				? nodes[nodes.length - 1].x + this.config.nodeWidth
				: this.config.nodeWidth;
		const maxDepth = Math.max(...nodes.map((n) => n.depth), 0);
		const height = (maxDepth + 1) * (this.config.nodeHeight + this.config.verticalSpacing);

		return { nodes, width, height };
	}
}

// Default instance
export const treeLayout = new TreeLayout();
