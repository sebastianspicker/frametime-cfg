# NVIDIA DRS settings

The NVIDIA DRS path is limited to an authenticated, exact NVIDIA adapter and the CS2 profile/application identity it records. It uses the pinned public NVAPI interface, captures supported prior settings before mutation, reloads and verifies the managed result, and restores only the recorded suite-owned changes.

The workflow does not claim that any driver setting is universally optimal. Driver version, game build, display mode, Reflex, VRR, and hardware determine the relevant result. NVIDIA hardware, NVAPI behavior, and driver interactions require Windows validation.

The settings path is inapplicable on non-NVIDIA systems and fails closed when identity, API loading, or persisted prerequisite evidence differs.
