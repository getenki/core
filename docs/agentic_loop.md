```
Algorithm AgentRun(session_id, user_message):

1. Load previous messages for session_id
   - If none exist, start with a system prompt

2. Append the new user_message to messages

3. Repeat up to max_iterations times:

   a. Send messages to the LLM
   b. Get assistant_message back
   c. Append assistant_message to messages

   d. Check whether assistant_message contains tool calls

   e. If tool calls exist:
      i.   Extract each tool name and its arguments
      ii.  Execute each tool
      iii. Convert each tool result into a message
      iv.  Append tool result messages to messages
      v.   Save session state
      vi.  Continue to next iteration

   f. Otherwise:
      i.   Read assistant_message.content as final text
      ii.  Save session state
      iii. Return final text

4. If an error happens:
   - Return "LLM error: ..."

5. If max_iterations is reached:
   - Save session state
   - Return "Max iterations reached."

```