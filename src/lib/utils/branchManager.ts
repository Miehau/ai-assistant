import type { ConversationTree, MessageTreeNode, Branch } from '$lib/types';

/**
 * Tree node for rendering
 */
export interface TreeNode {
	messageId: string;
	parentId: string | null;
	branchId: string;
	isBranchPoint: boolean;
	children: TreeNode[];
	depth: number;
	x: number; // Will be set by layout algorithm
	y: number; // Will be set by layout algorithm
}

/**
 * Branch manager for tree operations
 */
export class BranchManager {
	/**
	 * Build a tree structure from conversation tree data
	 */
	buildTree(tree: ConversationTree): TreeNode[] {
		const { nodes } = tree;

		// Create a map of message ID to tree node
		const nodeMap = new Map<string, TreeNode>();

		// First pass: create all nodes
		nodes.forEach((node) => {
			nodeMap.set(node.message_id, {
				messageId: node.message_id,
				parentId: node.parent_message_id,
				branchId: node.branch_id,
				isBranchPoint: node.branch_point,
				children: [],
				depth: 0,
				x: 0,
				y: 0
			});
		});

		// Second pass: build parent-child relationships
		const roots: TreeNode[] = [];
		nodeMap.forEach((node) => {
			if (node.parentId === null) {
				roots.push(node);
			} else {
				const parent = nodeMap.get(node.parentId);
				if (parent) {
					parent.children.push(node);
				}
			}
		});

		// Calculate depths
		const calculateDepth = (node: TreeNode, depth: number) => {
			node.depth = depth;
			node.children.forEach((child) => calculateDepth(child, depth + 1));
		};

		roots.forEach((root) => calculateDepth(root, 0));

		return roots;
	}

	/**
	 * Get the path from root to a specific message
	 */
	getPathToMessage(tree: ConversationTree, targetMessageId: string): string[] {
		const { nodes } = tree;
		const path: string[] = [];

		let currentId: string | null = targetMessageId;
		while (currentId !== null) {
			path.unshift(currentId);
			const node = nodes.find((n) => n.message_id === currentId);
			currentId = node?.parent_message_id ?? null;
		}

		return path;
	}

	/**
	 * Get all messages in a specific branch
	 */
	getBranchMessages(tree: ConversationTree, branchId: string): string[] {
		return tree.nodes.filter((n) => n.branch_id === branchId).map((n) => n.message_id);
	}

	/**
	 * Find all branch points (messages with multiple children)
	 */
	findBranchPoints(tree: ConversationTree): string[] {
		const childCounts = new Map<string, number>();

		tree.nodes.forEach((node) => {
			if (node.parent_message_id) {
				const count = childCounts.get(node.parent_message_id) || 0;
				childCounts.set(node.parent_message_id, count + 1);
			}
		});

		return Array.from(childCounts.entries())
			.filter(([_, count]) => count > 1)
			.map(([messageId, _]) => messageId);
	}

	/**
	 * Get children of a specific message
	 */
	getChildren(tree: ConversationTree, messageId: string): MessageTreeNode[] {
		return tree.nodes.filter((n) => n.parent_message_id === messageId);
	}

	/**
	 * Check if a message has multiple branches
	 */
	hasBranches(tree: ConversationTree, messageId: string): boolean {
		return this.getChildren(tree, messageId).length > 1;
	}

	/**
	 * Get branch count for a message
	 */
	getBranchCount(tree: ConversationTree, messageId: string): number {
		return this.getChildren(tree, messageId).length;
	}

	/**
	 * Get the branch for a specific message
	 */
	getMessageBranch(tree: ConversationTree, messageId: string): Branch | null {
		const node = tree.nodes.find((n) => n.message_id === messageId);
		if (!node) return null;

		return tree.branches.find((b) => b.id === node.branch_id) ?? null;
	}

	/**
	 * Compare two branches and find where they diverged
	 */
	findDivergencePoint(
		tree: ConversationTree,
		branchId1: string,
		branchId2: string
	): string | null {
		const messages1 = this.getBranchMessages(tree, branchId1);
		const messages2 = this.getBranchMessages(tree, branchId2);

		// Find common messages
		const common = messages1.filter((m) => messages2.includes(m));
		if (common.length === 0) return null;

		// Last common message is the divergence point
		return common[common.length - 1];
	}

	/**
	 * Get all descendant messages of a node
	 */
	getDescendants(tree: ConversationTree, messageId: string): string[] {
		const descendants: string[] = [];
		const queue = [messageId];

		while (queue.length > 0) {
			const current = queue.shift()!;
			const children = this.getChildren(tree, current);

			children.forEach((child) => {
				descendants.push(child.message_id);
				queue.push(child.message_id);
			});
		}

		return descendants;
	}

	/**
	 * Flatten tree to array for rendering
	 */
	flattenTree(roots: TreeNode[]): TreeNode[] {
		const result: TreeNode[] = [];

		const traverse = (node: TreeNode) => {
			result.push(node);
			node.children.forEach(traverse);
		};

		roots.forEach(traverse);
		return result;
	}
}

// Singleton instance
export const branchManager = new BranchManager();
