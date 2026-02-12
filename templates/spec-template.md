# Technical Specification: [Feature or Epic Title]

**Status:** Draft | In Review | Approved
**Author:** [Your Name]
**Created:** [YYYY-MM-DD]
**PRD Link:** [Link to corresponding PRD or epic]

---

## Architecture Overview

[Provide a high-level architectural description of how the feature will be implemented. Include:
- System components and their relationships
- Data flow and interactions
- Key design decisions and rationale
- Before/after diagrams or descriptions if helpful]

### Before/After (Optional)

**Before:**
[Current state or behavior]

**After:**
[New state or behavior after implementation]

---

## File Changes

[List each file or component that will be modified. Use numbered sections for each file.]

### 1. [Component/File Name]

**Location:** `path/to/file.rs` or `path/to/module/`

**Change Description:**
[Describe what will change in this file and why]

**Before:**
```
[Current code or structure]
```

**After:**
```
[New code or structure]
```

---

### 2. [Another Component/File Name]

**Location:** `path/to/another/file.ext`

**Change Description:**
[Describe the modification]

**Before:**
```
[Current implementation]
```

**After:**
```
[New implementation]
```

---

## Implementation Phases

[This section is critical - the `decompose` CLI command reads these phases to create beads tasks. Structure each phase with clear task lists and dependencies.]

### Phase 1: [Title]

**Description:** [What this phase accomplishes]

**Tasks:**
- [ ] [T1-1] Task description with estimated scope
- [ ] [T1-2] Subtask or related work
- [ ] [T1-3] Complete this phase requirement

**Dependencies:** [Other phases or tasks that must be completed first]

**Acceptance Criteria:**
- [Verifiable criterion for phase completion]
- [Test or validation requirement]

---

### Phase 2: [Title]

**Description:** [What this phase accomplishes]

**Tasks:**
- [ ] [T2-1] Task description
- [ ] [T2-2] Related implementation work
- [ ] [T2-3] Integration or testing tasks

**Dependencies:** Phase 1 (must complete Phase 1 before starting this phase)

**Acceptance Criteria:**
- [Criterion for phase completion]
- [Verification or test requirement]

---

### Phase 3: [Title]

**Description:** [What this phase accomplishes]

**Tasks:**
- [ ] [T3-1] Final implementation tasks
- [ ] [T3-2] Verification and validation
- [ ] [T3-3] Documentation or cleanup

**Dependencies:** Phase 2

**Acceptance Criteria:**
- [Final acceptance criterion]
- [Complete implementation check]

---

## Configuration

[Document any environment variables, settings, configuration files, or prerequisites needed.]

### Environment Variables

| Variable | Type | Required | Description |
|----------|------|----------|-------------|
| `VAR_NAME` | string | Yes | [What this variable controls] |
| `ANOTHER_VAR` | integer | No | [Optional configuration, default value] |

### Prerequisites

| Item | Description | How to Setup |
|------|-------------|--------------|
| [Dependency Name] | [What it is] | [Steps to install/configure] |
| [Tool or Library] | [Purpose in implementation] | [Installation or setup command] |

---

## Testing Strategy

[Describe how the implementation will be tested. Include different testing approaches.]

### 1. Unit Tests

[Describe unit tests for individual functions/modules]
- Test coverage targets: [percentage or specific modules]
- Mock dependencies: [What will be mocked]
- Test file location: `tests/` or `src/`

### 2. Integration Tests

[Describe integration tests for component interactions]
- Component combinations to test
- Setup and teardown requirements
- Expected behavior verification

### 3. End-to-End Tests

[Describe E2E tests for complete features]
- User workflows to validate
- Test data and scenarios
- Performance or load testing if applicable

### 4. Manual Testing Checklist

- [ ] [Manually test feature X]
- [ ] [Verify behavior in condition Y]
- [ ] [Check compatibility with existing features]
- [ ] [Validate error handling scenarios]

---

## Rollback Plan

[Describe how to revert these changes if problems occur in production.]

### Rollback Steps

1. [First step to revert - e.g., revert commit or feature flag]
2. [Stop/restart services if needed]
3. [Restore database state or data migrations]
4. [Verify rollback completed successfully]

### Rollback Verification

- [ ] [Check that system is in prior state]
- [ ] [Verify no data loss or corruption]
- [ ] [Confirm performance is back to baseline]
- [ ] [Validate no side effects remain]

### Known Limitations

[Document any limitations or risks in rollback:
- Irreversible changes (database migrations that can't be reversed)
- Manual intervention required
- Data considerations during rollback]

---

## Notes

[Any additional context, decisions, or considerations that don't fit other sections:]

- [Design decision and rationale]
- [Architectural alternative considered and why it wasn't chosen]
- [Known technical debt or future improvements]
- [References to spike/investigation work done during planning]

---

<!--
INSTRUCTIONS FOR USING THIS TEMPLATE:

1. Replace all [bracketed placeholders] with actual content
2. Update Status as you progress (Draft → In Review → Approved)
3. Link to the PRD that defines the what/why for this spec
4. Make Architecture Overview clear enough for anyone to understand the design
5. Be specific in File Changes with actual code examples
6. Implementation Phases MUST have clear task lists - this is what `decompose` reads
7. Include realistic dependencies between phases
8. Configuration section should be comprehensive and include examples
9. Testing Strategy should cover all critical paths and edge cases
10. Rollback Plan must be concrete and testable

For decompose integration:
- Create a PRD linked in the metadata
- Structure Implementation Phases with numbered tasks [T-Phase-Task format]
- List clear dependencies between phases
- Run `bd decompose <spec>` to generate beads tasks from phases
- Each phase becomes a beads epic, each task becomes a beads task
- Dependencies translate to beads blocking relationships

Tips:
- Keep phases logically grouped around deliverables
- Make task descriptions small enough to complete in one session
- Include acceptance criteria for each phase so you know when it's done
- Reference this spec in beads issues for full traceability
-->
