# PRD: [Feature or Epic Title]

**Status:** Draft | In Review | Approved
**Author:** [Your Name]
**Created:** [YYYY-MM-DD]
**Beads Epic:** [epic-id]

---

## Overview

[Write 1-2 paragraphs summarizing the feature or epic. Include:
- What problem this solves
- Why it matters now
- High-level approach or solution
- Key stakeholders or users affected]

## Goals

1. [Primary goal - what success looks like]
2. [Secondary goal - supporting outcome]
3. [Additional goals as needed]

### Non-Goals

- [What's explicitly out of scope for this work]
- [Features or changes that won't be addressed]
- [Common expectations to set boundaries around]

## User Stories

**US-1:** As a [role/user type], I want to [action/capability] so that I can [benefit/outcome].

**US-2:** As a [role/user type], I want to [action/capability] so that I can [benefit/outcome].

**US-3:** As a [role/user type], I want to [action/capability] so that I can [benefit/outcome].

[Add more user stories as needed. Use US-N numbering format.]

## Functional Requirements

**FR-1: [Requirement Title]**
- [Detailed description of what needs to be built]
- [Technical details, API signatures, configuration options]
- [Example code or usage patterns if applicable]

```
[Code examples or configuration snippets]
```

**FR-2: [Requirement Title]**
- [Description and technical details]
- [Specify behavior, edge cases, error handling]

**FR-3: [Requirement Title]**
- [Continue numbering for additional requirements]

[Add more functional requirements as needed. Use FR-N numbering format.]

## Technical Constraints

[List any technical limitations, compatibility requirements, or system constraints:
- Platform or language version requirements
- Performance targets or resource limits
- Dependencies on other systems or services
- Security or compliance requirements
- Architectural constraints]

## Success Criteria

1. [Measurable outcome - include specific metrics or tests]
2. [Verification method - how to confirm it works]
3. [Performance target - response time, throughput, etc.]
4. [User-facing criteria - what users should be able to do]

[Make these objective and testable. Avoid subjective criteria.]

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| [Description of potential risk] | [High/Medium/Low - what breaks if this happens] | [Specific actions to prevent or handle the risk] |
| [Technical risk - performance, scalability, etc.] | [Impact on users or system] | [Technical solution or workaround] |
| [Process risk - unclear requirements, dependencies] | [Impact on timeline or quality] | [Process change or clarification needed] |

## Out of Scope (Future Phases)

- [Features deferred to later releases]
- [Nice-to-have capabilities not required for MVP]
- [Related work that should be separate initiatives]
- [Known limitations to address in future iterations]

## References

- [Link to related issues or beads tasks]
- [Design documents or technical specs]
- [User research or data supporting the need]
- [External documentation or API references]
- [Prior art or similar implementations]

---

<!--
INSTRUCTIONS FOR USING THIS TEMPLATE:

1. Replace all [bracketed placeholders] with actual content
2. Update Status as you progress (Draft → In Review → Approved)
3. Link to the beads epic ID that tracks this work
4. Keep user stories focused on user value, not implementation
5. Make functional requirements specific and testable
6. Ensure success criteria are measurable
7. Be explicit about what's out of scope to avoid scope creep
8. You can omit sections that don't apply to your use case
9. Add custom sections if your project needs them

For beads integration:
- Create an epic with `bd create --type=epic`
- Link this PRD to the epic ID in the metadata
- Break down FRs into individual beads tasks
- Use dependencies to track blocking relationships
-->
