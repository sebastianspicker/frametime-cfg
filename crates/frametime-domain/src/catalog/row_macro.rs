macro_rules! s {
    ($p:ident, $n:literal, $c:literal, $t:literal, $tier:literal, $r:ident, $d:ident, $intent:ident, $check:literal, $reboot:literal) => {
        Step {
            id: StepId::new(Phase::$p, $n),
            category: $c,
            title: $t,
            tier: $tier,
            risk: Risk::$r,
            depth: Depth::$d,
            check_only: $check,
            reboot: $reboot,
            intent: ActionIntent::$intent,
            operation: operation_kind(Depth::$d),
            gpu_applicability: gpu_applicability(StepId::new(Phase::$p, $n)),
            orchestration_role: orchestration_role(StepId::new(Phase::$p, $n)),
        }
    };
}
