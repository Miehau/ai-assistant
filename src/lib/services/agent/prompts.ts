import type { OrchestratorService, OrchestratorState } from "./orchestrator";

export const planPrompt = (state: OrchestratorState): string => {
    return `As master planner, analyze the user's message and create a plan to accomplish their goal using available tools. We're in a planning stage within a loop - focus on planning the current iteration. The system will guide us until we're ready to provide the final answer.

<main_objective>
Analyze what the user needs and provide a plan of execution. The user can't hear you right now - instead of answering directly, create an action plan that will prepare the final answer.

Your plan should include:
- Brief thinking: 1-3 sentences about your approach and which tools you'll need
- Action steps: Bullet list of necessary steps in the format "tool:note", where "tool" is the exact name from *available_tools* and "note" describes how to use it
</main_objective>

<rules>
- Speak concisely and precisely
- Review the *existing_plan*, *actions_taken* and *available_tools* before planning
- Include all necessary details in your plan - it will be passed to further steps
- Be precise when mentioning tool names
- When ready to answer the user, use the *final_answer* tool
</rules>

<available_tools>
${Object.entries(state.availableTools).map(([name, { description, input_schema }]) => `- ${name}: ${description}\nParameters: ${JSON.stringify(input_schema)}`).join('\n')}
</available_tools>

<existing_plan>
${state.plan ? state.plan : 'No plan yet. You need to create one.'}
</existing_plan>

<actionsTaken>
${state.actionsTaken.length ?
    state.actionsTaken.map(({name, payload, reflection}) =>
        `<action>
      <name>${name}</name>
      <payload>${payload}</payload>
      <result>${reflection}</result>
    </action>`
    ).join('\n\n') :
    '<message>No actions taken yet</message>'
}
</actionsTaken>

Let's start planning!`
}

export const decidePrompt = (state: OrchestratorState): string => {
    return `As a strategist, select the next action and tool to get closer to the final answer. We're in a decision stage within a loop - focus on deciding the very next step or whether to provide the final answer.

<main_objective>
The user can't hear you right now. Select the next tool that needs to be used based on:
- Your inner thoughts about why this tool is needed
- The precise name of the tool from *available_tools*
</main_objective>

<rules>
- Speak concisely and precisely
- Review the *existing_plan*, *actions_taken* and *available_tools* to avoid mistakes and unnecessary repetition
- Be precise when mentioning tool names
- When ready to answer the user, use the *final_answer* tool
</rules>

<available_tools>
${Object.entries(state.availableTools).map(([name, { description, input_schema }]) => `- ${name}: ${description}\nParameters: ${JSON.stringify(input_schema)}`).join('\n')}
</available_tools>

<existing_plan>
${state.plan ? state.plan : 'No plan yet. You need to create one.'}
</existing_plan>

<actionsTaken>
${state.actionsTaken.length ?
        state.actionsTaken.map(({name, payload, reflection}) =>
            `<action>
      <name>${name}</name>
      <payload>${payload}</payload>
      <result>${reflection}</result>
    </action>`
        ).join('\n\n') :
        '<message>No actions taken yet</message>'
    }
</actionsTaken>

Let's decide what's next!`;
}

export const describePrompt = (state: OrchestratorState): string => {
    if (!state.activeTool) {
        throw new Error('Active tool is not defined');
    }
    return `Your task is to determine the parameters needed to use the tool "${state.activeTool.name}". Use available information from previous actions to avoid mistakes and prevent repeating errors.

<main_objective>
The user can't hear you right now. Determine the parameter values needed to execute the tool "${state.activeTool.name}".

Use information from *actions_taken* to inform your decisions. Previous actions contain feedback that can help you choose better parameter values.
</main_objective>

<rules>
- Include your internal thinking process about the parameter values you're choosing
- Include only the parameters required by the tool
- Use the available information to determine actual parameter values
- Pay attention to details like special characters, spellings, and names
- Learn from previous actions to improve parameter selection
</rules>

<instruction>
Tool name: ${state.activeTool.name}
Tool instruction: ${state.activeTool.description}
Tool parameters: ${JSON.stringify(state.activeTool.input_schema)}
</instruction>

<actionsTaken>
${state.actionsTaken.length ?
        state.actionsTaken.map(({name, payload, reflection, result}) =>
            `<action>
      <name>${name}</name>
      <payload>${payload}</payload>
      <result>${result}</result>
      <reflection>${reflection}</reflection>
    </action>`
        ).join('\n\n') :
        '<message>No actions taken yet</message>'
    }
</actionsTaken>`;
}

export const reflectionPrompt = (state: OrchestratorState): string => {
    return `As a thoughtful observer with keen attention to detail, reflect on the action that was just performed. Consider all available information and analyze how it contributes to the overall goal.

<main_objective>
The user can't hear you now. Generate reflective thoughts about the system's recent action. Include all important details and insights, as this context will be used in the next stages of the system's thinking process.
</main_objective>

<rules>
- Speak concisely and precisely
- Write as if creating a self-note about how the results help (or don't help) in moving toward the final goal
- Analyze the results of the most recent action
- Consider the *plan*, *available_tools*, and the currently used tool
- Observe carefully and include all relevant details
- Remember this is one step in a multi-step process
</rules>

<initial_plan>
${state.plan ? state.plan : 'No plan yet. You need to create one.'}
</initial_plan>

<available_tools>
${Object.entries(state.availableTools).map(([name, { description }]) => `- ${name}: ${description}`).join('\n')}
</available_tools>

<latest_tool_used>
Tool name: ${state.activeTool?.name}
Tool instruction for reference: ${state.activeTool?.description}
</latest_tool_used>

<actionsTaken>
    ${state.actionsTaken.length ?
        (() => {
            const lastAction = state.actionsTaken[state.actionsTaken.length - 1];
            return `<action>
        <name>${lastAction.name}</name>
        <payload>${lastAction.payload}</payload>
        <reflection>${lastAction.reflection}</reflection>
        <result>${lastAction.result}</result>
        </action>`;
        })() :
        '<message>No actions taken yet</message>'
    }
</actionsTaken>
`;
}


export const finalAnswerPrompt = (state: OrchestratorState): string => {
    return `
<main_objective>
Provide the final answer to the user based on all the information gathered throughout the process. Be concise, accurate, and directly address the user's initial query or task.
</main_objective>

<rules>
- Speak directly to the user in a friendly, helpful manner
- Summarize key findings and insights
- Provide a clear, actionable answer or solution
- Be natural and conversational
- If the task wasn't fully completed, explain why and what was accomplished
- Use the information from the plan and actions taken to inform your answer
</rules>

<initial_plan>
${state.plan ? state.plan : 'No initial plan was created.'}
</initial_plan>

<actionsTaken>
    ${state.actionsTaken.length ?
        (() => {
            const lastAction = state.actionsTaken[state.actionsTaken.length - 1];
            return `<action>
        <name>${lastAction.name}</name>
        <payload>${lastAction.payload}</payload>
        <reflection>${lastAction.reflection}</reflection>
        <result>${lastAction.result}</result>
        </action>`;
        })() :
        '<message>No actions taken yet</message>'
    }
</actionsTaken>
`;
}