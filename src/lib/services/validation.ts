import { z } from 'zod';

export const createToolSchema = <T extends z.ZodType>(schema: T) => schema;

export const validateToolParameters = <T>(
	schema: z.ZodType<T>,
	params: unknown
): { success: true; data: T } | { success: false; error: string } => {
	const result = schema.safeParse(params);
	if (result.success) {
		return { success: true, data: result.data };
	}
	return {
		success: false,
		error: result.error.issues.map((e: z.ZodIssue) => `${e.path.join('.')}: ${e.message}`).join(', ')
	};
};
