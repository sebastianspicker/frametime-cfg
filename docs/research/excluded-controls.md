# Excluded and conditional controls

The project omits or treats many historical tuning controls as conditional because their current behavior, target identity, recovery, or measurable benefit is insufficiently established.

Examples include legacy game launch flags, blanket timer or scheduler changes, generic TCP tuning for a UDP game workload, indiscriminate cache deletion, and universal driver profiles. Exclusion is not proof that a control can never matter; it means the native workflow will not present it as a default optimization claim.

New controls need a clear ownership boundary, validation and recovery plan, and current evidence before entering a state-changing workflow.
