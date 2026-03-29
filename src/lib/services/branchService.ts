import type {
	Branch,
	MessageTreeNode,
	ConversationTree,
	BranchPath,
	BranchStats
} from '$lib/types';

/**
 * Service for managing conversation branches
 * Wraps backend commands for branch operations
 */
export class BranchService {
	/**
	 * Create a new branch in a conversation
	 */
	async createBranch(conversationId: string, name: string): Promise<Branch> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Create a message tree node linking a message to its parent and branch
	 */
	async createMessageTreeNode(
		messageId: string,
		parentMessageId: string | null,
		branchId: string,
		isBranchPoint: boolean
	): Promise<MessageTreeNode> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Get all branches for a conversation
	 */
	async getConversationBranches(conversationId: string): Promise<Branch[]> {
		console.warn('[branchService] getConversationBranches not yet implemented in server backend');
		return [];
	}

	/**
	 * Get the complete conversation tree structure
	 */
	async getConversationTree(conversationId: string): Promise<ConversationTree> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Get a specific branch path with its messages
	 */
	async getBranchPath(branchId: string): Promise<BranchPath> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Rename a branch
	 */
	async renameBranch(branchId: string, newName: string): Promise<void> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Delete a branch
	 */
	async deleteBranch(branchId: string): Promise<void> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Get branch statistics for a conversation
	 */
	async getBranchStats(conversationId: string): Promise<BranchStats> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Get or create the main branch for a conversation
	 */
	async getOrCreateMainBranch(conversationId: string): Promise<Branch> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Create a new branch from a specific message in the conversation
	 */
	async createBranchFromMessage(
		conversationId: string,
		parentMessageId: string,
		branchName: string
	): Promise<Branch> {
		throw new Error('Not yet implemented in server backend');
	}

	/**
	 * Generate an auto-incrementing branch name
	 */
	async generateBranchName(conversationId: string): Promise<string> {
		const branches = await this.getConversationBranches(conversationId);
		const branchNumbers = branches
			.map((b) => {
				const match = b.name.match(/^Branch (\d+)$/);
				return match ? parseInt(match[1]) : 0;
			})
			.filter((n) => n > 0);

		const nextNumber = branchNumbers.length > 0 ? Math.max(...branchNumbers) + 1 : 1;
		return `Branch ${nextNumber}`;
	}

	/**
	 * Check message tree consistency and identify orphaned messages
	 */
	async checkMessageTreeConsistency(): Promise<any> {
		console.warn('[branchService] checkMessageTreeConsistency not yet implemented in server backend');
		return {};
	}

	/**
	 * Repair message tree by adding orphaned messages to their conversation's main branch
	 */
	async repairMessageTree(): Promise<number> {
		throw new Error('Not yet implemented in server backend');
	}
}

// Singleton instance
export const branchService = new BranchService();
