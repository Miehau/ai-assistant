pub const TRIAGE_PROMPT: &str = r#"You are an AI assistant analyzing a user request. Determine the appropriate response strategy.

USER REQUEST:
{user_message}

CONVERSATION CONTEXT:
{recent_messages}

Analyze this request and respond with JSON:

{{
  "type": "triage",
  "decision": "direct_response" | "needs_clarification" | "ready_to_plan",
  "response": "...",
  "reasoning": "..."
}}

Guidelines:
- "direct_response": Simple questions, greetings, factual queries that need no tools
- "needs_clarification": Ambiguous requests, missing critical information
- "ready_to_plan": Clear task that requires tool usage or multiple steps
"#;

pub const CLARIFY_PROMPT: &str = r#"You are gathering information before creating an execution plan.

USER REQUEST:
{user_message}

INFORMATION GATHERED SO FAR:
{gathered_info}

What additional information do you need? Respond with JSON:

{{
  "type": "clarify",
  "questions": ["..."],
  "assumptions": ["..."],
  "needs_user_input": false,
  "reasoning": "..."
}}

Guidelines:
- Only set needs_user_input=true if information CANNOT be obtained any other way
- Prefer making reasonable assumptions over blocking on user input
- Questions should be specific and actionable
"#;

pub const PLAN_PROMPT: &str = r#"Create a step-by-step execution plan for the user's request.

USER REQUEST:
{user_message}

GATHERED INFORMATION:
{gathered_info}

AVAILABLE TOOLS:
{tool_descriptions}

Create a plan with concrete steps. Respond with JSON:

{{
  "type": "plan",
  "goal": "...",
  "assumptions": ["..."],
  "steps": [
    {{
      "id": "step-1",
      "description": "...",
      "expected_outcome": "...",
      "action": {{ "tool": "tool_name", "args": {{...}} }}
    }}
  ]
}}

Guidelines:
- Each step should have exactly ONE action (tool call OR ask_user OR think)
- Steps should be atomic and verifiable
- Expected outcomes should be specific and measurable
- Order steps logically (dependencies first)
"#;

pub const REFLECT_PROMPT: &str = r#"Evaluate the result of the last executed step and decide how to proceed.

PLAN GOAL:
{plan_goal}

CURRENT STEP:
{step_description}

EXPECTED OUTCOME:
{expected_outcome}

ACTUAL RESULT:
{step_result}

REMAINING STEPS:
{remaining_steps}

Analyze the result and decide next action. Respond with JSON:

{{
  "type": "reflect",
  "decision": "continue" | "adjust" | "need_more_info" | "done" | "need_human_input",
  "reason": "...",
  "new_steps": [...],
  "summary": "...",
  "question": "..."
}}

Guidelines:
- "continue": Result matches expected outcome, proceed to next step
- "adjust": Result requires changing remaining steps
- "need_more_info": Need to gather more information before continuing
- "done": Task is complete, no more steps needed
- "need_human_input": Cannot proceed without human decision
"#;
