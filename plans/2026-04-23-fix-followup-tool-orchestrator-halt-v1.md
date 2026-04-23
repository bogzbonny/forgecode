# Fix: Followup Tool Should Not Halt Orchestrator

## Objective

When the LLM calls the `followup` tool, the orchestrator currently exits the main conversation loop and returns control to the REPL prompt. The expected behavior is that after the user answers the follow-up question, the orchestrator should automatically continue the conversation loop with the updated context (including the user's answer), allowing the LLM to process the answer and continue the conversation without requiring the user to manually re-submit their response.

## Root Cause Analysis

The problem is in the orchestrator loop in `crates/forge_main/src/app/orch.rs:228-439`. Here is the flow:

1. **LLM calls `followup` tool** - The orchestrator receives a response containing a `followup` tool call.

2. **Tool execution** - The `followup` tool is executed via `execute_tool_calls()` at line 322-324, which captures the user's answer.

3. **Yield detection** - At lines 315-319, `should_yield` is set to `true` because `ToolCatalog::should_yield("followup")` returns `true`:
   ```rust
   should_yield = is_complete
       || message.tool_calls.iter().any(|call| ToolCatalog::should_yield(&call.name));
   ```

4. **Context update** - The tool call records (including the user's answer) are appended to the context at lines 349-357.

5. **End hook fires** - At lines 407-428, the `End` hook fires. Since the `PendingTodosHandler` doesn't add messages (no pending todos), `should_yield` remains `true`.

6. **Loop exits** - The `while !should_yield` loop exits at line 256.

7. **No TaskComplete** - At lines 434-436, `ChatResponse::TaskComplete` is NOT sent because `is_complete` is `false` (there were tool calls).

8. **Stream ends** - The orchestrator returns `Ok(())`, the stream ends, and the UI's `on_chat()` returns.

9. **REPL resumes** - The main REPL loop calls `self.prompt()` again, waiting for user input.

**The key issue**: After the user answers the follow-up question, the orchestrator exits the loop instead of continuing with the updated context. The user's answer IS in the context (appended at step 4), but the orchestrator never sends this updated context back to the LLM.

**Evidence from tests**: The test `test_followup_does_not_trigger_session_summary` in `crates/forge_main/src/app/orch_spec/orch_spec.rs:66-95` confirms that `TaskComplete` is NOT sent for followup, but it also confirms that the orchestrator exits the loop (the test uses 2 mock responses: one with the followup call, one with a stop response).

**ToolCatalog definition**: `ToolCatalog::should_yield()` at `crates/forge_main/src/domain/tools/catalog.rs:898-904` only returns `true` for `ToolKind::Followup`, confirming `followup` is the sole yield tool.

## Implementation Plan

- [ ] **Task 1: Track whether yield is due to a followup tool call**
  - **Rationale**: We need to distinguish between yielding due to `followup` (which should continue the loop) vs yielding due to task completion (which should exit). Currently, `should_yield` conflates these two cases.
  - Add a new boolean variable `followup_yield` that is set to `true` when any tool call in the current response is a `followup` tool. This is computed using `ToolCatalog::should_yield()` on each tool call name.
  - This variable should be set alongside `should_yield` and `is_complete` in the orchestrator loop.

- [ ] **Task 2: Modify the End hook handling to continue on followup yield**
  - **Rationale**: The current code at lines 407-428 checks if the End hook added messages to decide whether to continue. We need to add a second condition: if the yield is due to a `followup` tool call, always continue the loop regardless of whether the End hook added messages.
  - In the `if should_yield` block (lines 407-428), add logic: if `followup_yield` is `true`, set `should_yield = false` to continue the loop. The updated context (which now includes the user's answer from the tool result) will be sent back to the LLM in the next iteration.
  - The End hook should still fire for `followup` yields (for any side effects), but the loop should continue.

- [ ] **Task 3: Prevent TaskComplete from being sent on followup yield**
  - **Rationale**: At lines 434-436, `TaskComplete` is only sent when `is_complete` is `true`. Since `is_complete` is `false` when `followup` is called, `TaskComplete` is already NOT sent. However, we should add an explicit check to ensure `TaskComplete` is never sent when `followup_yield` is `true`, as a safety measure.
  - Update the condition at line 434 to: `if is_complete && !followup_yield { ... }`

- [ ] **Task 4: Update the test `test_followup_does_not_trigger_session_summary`**
  - **Rationale**: The existing test at `orch_spec.rs:66-95` verifies that `TaskComplete` is NOT sent for followup, but it also verifies that the orchestrator uses 2 mock responses (one with followup, one with stop). After the fix, the orchestrator should continue the loop and produce a second response naturally. We need to ensure the test still passes and accurately reflects the new behavior.
  - Verify the test still asserts that `TaskComplete` is not sent.
  - Verify the test still asserts that tools are in the context.
  - The mock response setup should remain the same (2 responses), but the test should now verify that the orchestrator continues the loop after the followup tool call.

- [ ] **Task 5: Add a new test for followup tool continuation**
  - **Rationale**: We need explicit test coverage for the new behavior: the orchestrator should continue the conversation loop after a `followup` tool call, processing the user's answer and allowing the LLM to respond.
  - Create a test that mocks: (a) an assistant response with a `followup` tool call, (b) a tool result with the user's answer, (c) a second assistant response that processes the answer and completes the task.
  - Verify that the orchestrator produces 2 assistant messages (one with followup, one with completion).
  - Verify that `TaskComplete` IS sent after the final response.
  - Verify that the context contains both the followup tool call and the tool result.

## Verification Criteria

- [ ] Running `cargo insta test --accept` in the `forge_main` crate passes all existing tests
- [ ] The new test `test_followup_continues_conversation` passes
- [ ] The existing test `test_followup_does_not_trigger_session_summary` still passes
- [ ] When the LLM calls `followup`, the orchestrator continues the loop after the tool result is processed
- [ ] No `ChatResponse::TaskComplete` is sent after a `followup` tool call
- [ ] `ChatResponse::TaskComplete` IS sent after the LLM completes the task (no tool calls, finish_reason=Stop)

## Potential Risks and Mitigations

1. **[Risk: Infinite loop if LLM keeps calling followup]**
   - **Mitigation**: The existing `max_requests_per_turn` limit (checked at lines 377-398) will prevent infinite loops. If the LLM keeps calling `followup`, it will eventually hit the request limit and trigger an `Interrupt` with `MaxRequestPerTurnLimitReached`.

2. **[Risk: End hook side effects on followup yield]**
   - **Mitigation**: The `End` hook already handles this gracefully. The `DoomLoopDetector` and `PendingTodosHandler` hooks check the conversation state and only add messages when appropriate. They will not interfere with the followup continuation flow.

3. **[Risk: Tool result not properly appended to context]**
   - **Mitigation**: The tool call records are appended to the context at lines 349-357, BEFORE the `should_yield` check. This means the user's answer from the `followup` tool result is already in the context when the loop continues. No changes needed here.

4. **[Risk: UI behavior change affects user experience]**
   - **Mitigation**: The UI already handles the case where the orchestrator continues the loop. The `on_chat()` method streams responses until the orchestrator returns. The only change is that the stream will be longer (one more LLM response) instead of ending immediately after the `followup` tool call.

## Alternative Approaches

1. **[Alternative 1: Use a new ChatResponse variant for followup]**
   - Instead of modifying the orchestrator loop, introduce a new `ChatResponse::FollowupAnswer` variant that signals the UI to automatically re-prompt the orchestrator with the user's answer.
   - **Trade-offs**: More complex UI changes, requires new streaming message handling. The orchestrator loop modification is cleaner and more centralized.

2. **[Alternative 2: Modify ToolCatalog::should_yield() to not include followup]**
   - Remove `Followup` from the yield tools list, and handle the yield behavior differently (e.g., via a new tool-specific attribute).
   - **Trade-offs**: Breaks the existing `should_yield` contract. The `should_yield` mechanism is used to signal that the orchestrator should pause and wait for external input. `followup` is the canonical example of this pattern.

3. **[Alternative 3: Handle continuation in the UI layer]**
   - After the orchestrator exits, the UI could automatically call `on_message(None)` to continue the conversation.
   - **Trade-offs**: This would require the UI to know about the `followup` tool and its result, which couples the UI to the tool implementation. The orchestrator loop modification keeps the continuation logic in the orchestrator where it belongs.
