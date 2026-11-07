# Write an implementation plan

Investigate the request and relevant code, then write an actionable plan for implementing the feature or feature set. Do not implement it yet.

The plan must contain:

- Clear goals, scope, and immutable acceptance criteria.
- Design rules and important constraints.
- Staged implementation steps using checkbox lists, with verification for each stage.
- A final end-to-end validation stage.

Unless instructed otherwise, follow these guidelines:
- include branch creation, pushing, creating a PR and working to ensure CI turns green.
- include checking the PR for any automated review coming in. If so, repeatedly address review feedback as you see fit, until the reviewer approves.

Make dependencies and ordering explicit, but keep the plan concise and avoid speculative detail.

Include this execution contract in the plan:

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

When asked to write a plan according to these guidelines, after writing, include in your response to the requestor:

- one-line summary per stage
- summary of the testing plan - in particular how you are balancing testing time vs. confidence at each step
- summary of whether and to what degree your plan includes checking CI and/or code review and/or autonomous PR merging
